//! Command-line tool for the `chi` content-coding.

use std::{
    fs::File,
    io::{self, BufWriter, Read, Write},
    iter::repeat,
    process::ExitCode,
};

use chic::{Decoder, Digest, sync};

/// The chiclet content length that every implementation is required to support.
const DEFAULT_CHICLET_SIZE: u64 = (1 << 14) - 1;

const USAGE: &str = "\
usage: chic encode [--size N | --lengths A,B,...] FILE   (digest is printed to stderr)
       chic digest [--size N | --lengths A,B,...] FILE
       chic decode --digest DIGEST [--max N] FILE

DIGEST is a structured field Byte Sequence (`:base64:`), optionally prefixed with `chi=`.
--size, --lengths, and --max count content bytes only, excluding length fields and hashes.";

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(B64[(v >> (18 - 6 * i)) as usize & 0x3f]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn unbase64(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = B64.iter().position(|&b| b == c)?;
        acc = (acc << 6) | u32::try_from(v).ok()?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).ok()?);
        }
    }
    Some(out)
}

fn parse_digest(s: &str) -> Option<Digest> {
    let s = s.strip_prefix("chi=").unwrap_or(s);
    let s = s
        .strip_prefix(':')
        .and_then(|s| s.strip_suffix(':'))
        .unwrap_or(s);
    unbase64(s)?.try_into().ok()
}

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("chic: {msg}");
    ExitCode::FAILURE
}

#[derive(Default)]
struct Args {
    size: Option<u64>,
    lengths: Option<Vec<u64>>,
    digest: Option<Digest>,
    max: Option<u64>,
    file: Option<String>,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut out = Self::default();
        while let Some(a) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{a} requires a value"));
            match a.as_str() {
                "--size" => out.size = Some(value()?.parse().map_err(|e| format!("--size: {e}"))?),
                "--max" => out.max = Some(value()?.parse().map_err(|e| format!("--max: {e}"))?),
                "--lengths" => {
                    let v = value()?
                        .split(',')
                        .map(str::parse)
                        .collect::<Result<_, _>>()
                        .map_err(|e| format!("--lengths: {e}"))?;
                    out.lengths = Some(v);
                }
                "--digest" => {
                    out.digest = Some(parse_digest(&value()?).ok_or("invalid digest")?);
                }
                _ if out.file.is_none() && !a.starts_with("--") => out.file = Some(a),
                _ => return Err(format!("unexpected argument: {a}")),
            }
        }
        if out.size.is_some() && out.lengths.is_some() {
            return Err("--size and --lengths are mutually exclusive".into());
        }
        Ok(out)
    }

    fn lengths(&self) -> Box<dyn Iterator<Item = u64>> {
        if let Some(l) = &self.lengths {
            Box::new(l.clone().into_iter())
        } else {
            Box::new(repeat(self.size.unwrap_or(DEFAULT_CHICLET_SIZE)))
        }
    }

    fn open(&self) -> io::Result<File> {
        File::open(self.file.as_deref().unwrap_or("/dev/stdin"))
    }
}

fn run(cmd: &str, args: &Args) -> chic::Result<()> {
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    match cmd {
        "encode" => {
            let digest = sync::encode_file(&mut args.open()?, args.lengths(), &mut out)?;
            out.flush()?;
            eprintln!("chi=:{}:", base64(&digest));
        }
        "digest" => {
            let digest = sync::digest_file(&mut args.open()?, args.lengths())?;
            writeln!(out, "chi=:{}:", base64(&digest))?;
        }
        "decode" => {
            let digest = args
                .digest
                .ok_or(io::Error::other("--digest is required"))?;
            let decoder =
                Decoder::new(digest).with_max_length(args.max.unwrap_or(DEFAULT_CHICLET_SIZE));
            let mut r = sync::DecodeReader::with_decoder(args.open()?, decoder);
            let mut buf = vec![0; 1 << 16];
            loop {
                let n = r.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
            }
        }
        _ => unreachable!(),
    }
    out.flush()?;
    Ok(())
}

fn main() -> ExitCode {
    let mut argv = std::env::args().skip(1);
    let Some(cmd) = argv
        .next()
        .filter(|c| ["encode", "digest", "decode"].contains(&c.as_str()))
    else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let args = match Args::parse(argv) {
        Ok(a) => a,
        Err(e) => return fail(format!("{e}\n{USAGE}")),
    };
    match run(&cmd, &args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        for s in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(unbase64(&base64(s)).as_deref(), Some(s));
        }
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
    }
}
