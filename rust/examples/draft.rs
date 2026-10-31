//! Generates the example section of draft-thomson-httpbis-chic.
//!
//! Run with `cargo run --example draft` and paste the output into the draft.

use std::{fmt::Write as _, iter::repeat};

use chic::{Layout, LayoutHasher, sync::encode_slice};

const CONTENT: &[u8] = b"Each chiclet carries the hash of the next one.\n";
const SIZE: u64 = 16;
const COMMENT_COLUMN: usize = 38;

fn hex(data: &[u8]) -> String {
    data.chunks(4)
        .map(|c| {
            c.iter().fold(String::new(), |mut s, b| {
                write!(s, "{b:02x}").unwrap();
                s
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn base64(data: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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
                out.push(char::from(
                    B64[usize::try_from((v >> (18 - 6 * i)) & 0x3f).unwrap()],
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Render content as a quoted string, escaping newlines.
fn quoted(data: &[u8]) -> String {
    let s: String = String::from_utf8_lossy(data).escape_default().collect();
    format!("\"{s}\"")
}

/// Append a line of hex, with an optional comment.
fn line(out: &mut String, data: &[u8], comment: &str) {
    let h = hex(data);
    if comment.is_empty() {
        writeln!(out, "{h}").unwrap();
    } else {
        writeln!(out, "{h:COMMENT_COLUMN$}# {comment}").unwrap();
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "linear script that emits the draft text"
)]
fn main() {
    let total = u64::try_from(CONTENT.len()).unwrap();
    let layout = Layout::new(total, repeat(SIZE)).unwrap();
    let mut hasher = LayoutHasher::new(&layout);
    hasher.update(CONTENT);
    let chain = hasher.finish().unwrap();
    let (encoded, digest) = encode_slice(CONTENT, repeat(SIZE)).unwrap();
    assert_eq!(&digest, chain.digest());

    let n = layout.len();
    println!(
        "This example shows content of {} bytes, split into chiclets of {SIZE} bytes.",
        CONTENT.len()
    );
    println!("This chiclet size is far smaller than would be used in practice,");
    println!("but it allows the example to show multiple chiclets.");
    println!("The resulting {n} chiclets are shown in {{{{ex-chiclets}}}},");
    println!("with the hash of each, starting from the last.");
    println!();
    println!("~~~");
    let chiclets: Vec<_> = layout.iter().collect();
    for (i, c) in chiclets.iter().enumerate().rev() {
        let len = usize::try_from(c.len).unwrap();
        let off = usize::try_from(c.offset).unwrap();
        let length_field = if c.is_final { 0 } else { c.len };
        println!("Chiclet {}:", i + 1);
        println!("  Length: {length_field}");
        println!("  Content: {}", quoted(&CONTENT[off..off + len]));
        if !c.is_final {
            println!("  Hash of Chiclet {}:", i + 2);
            let h = chain.hash(i + 1).unwrap();
            println!("    {}", hex(&h[..16]));
            println!("    {}", hex(&h[16..]));
        }
        println!("  SHA-256 of Chiclet {}:", i + 1);
        let h = chain.hash(i).unwrap();
        println!("    {}", hex(&h[..16]));
        println!("    {}", hex(&h[16..]));
        if i > 0 {
            println!();
        }
    }
    println!("~~~");
    println!("{{: #ex-chiclets title=\"Example Chiclets\"}}");
    println!();
    println!("The hash of the first chiclet is carried in the Content-Digest field,");
    println!("as shown in the HTTP response in {{{{ex-response}}}}.");
    println!();
    println!("~~~ http-message");
    println!("HTTP/1.1 200 OK");
    println!("Content-Type: text/plain");
    println!("Content-Encoding: chi");
    println!("Content-Length: {}", encoded.len());
    println!("Content-Digest: chi=:{}:", base64(&digest));
    println!("~~~");
    println!("{{: #ex-response title=\"Example Response Header\"}}");
    println!();
    println!(
        "The {} bytes of response content are shown in {{{{ex-content}}}} in hexadecimal.",
        encoded.len()
    );
    println!();

    let mut out = String::new();
    let mut pos = 0;
    for (i, c) in layout.iter().enumerate() {
        let len = usize::try_from(c.len).unwrap();
        let header_len = if c.is_final {
            1
        } else {
            chic::varint::encoded_len(c.len)
        };
        let field = &encoded[pos..pos + header_len];
        let value = if c.is_final { 0 } else { c.len };
        line(&mut out, field, &format!("Length = {value}"));
        pos += header_len;
        for row in encoded[pos..pos + len].chunks(16) {
            line(&mut out, row, &quoted(row));
        }
        pos += len;
        if !c.is_final {
            let h = &encoded[pos..pos + chic::HASH_LEN];
            line(&mut out, &h[..16], &format!("Hash of Chiclet {}", i + 2));
            line(&mut out, &h[16..], "");
            pos += chic::HASH_LEN;
        }
    }
    assert_eq!(pos, encoded.len());
    println!("~~~");
    print!("{out}");
    println!("~~~");
    println!("{{: #ex-content title=\"Example Encoded Content\"}}");
    println!();
    println!("Empty content is encoded as a single final chiclet,");
    println!("comprising a single byte with a value of 0.");
    let (encoded, digest) = encode_slice(b"", repeat(SIZE)).unwrap();
    assert_eq!(encoded, [0]);
    println!("The `chi` digest of empty content is therefore the SHA-256 hash");
    println!("of that one byte, as shown in {{{{ex-empty}}}}.");
    println!();
    println!("~~~ http-message");
    println!("Content-Encoding: chi");
    println!("Content-Length: 1");
    println!("Content-Digest: chi=:{}:", base64(&digest));
    println!("~~~");
    println!("{{: #ex-empty title=\"Fields for Empty Content\"}}");
}
