---
title: "Chained Hash Integrity Coding for HTTP"
abbrev: "CHIC"
category: std

docname: draft-thomson-httpbis-chic-latest
submissiontype: IETF  # also: "independent", "editorial", "IAB", or "IRTF"
number:
date:
consensus: true
v: 3
area: ""
workgroup: "HTTP"
keyword:
 - next generation
 - unicorn
 - sparkling distributed ledger
venue:
  group: "HTTP"
  type: ""
  mail: "ietf-http-wg@w3.org"
  arch: "https://lists.w3.org/Archives/Public/ietf-http-wg/"
  github: "martinthomson/chic"
  latest: "https://martinthomson.github.io/chic/draft-thomson-httpbis-chic.html"

author:
 -
    fullname: "Martin Thomson"
    organization: Mozilla
    email: "mt@lowentropy.net"

normative:

informative:


--- abstract

This describes a content-coding for HTTP that enables progressive processing of content,
without sacrificing integrity.


--- middle

# Introduction

Hash-based integrity is a powerful tool for ensuring that content matches expectations.
Hashes are often the basis of digital signature schemes
that can authenticate content,
or at least provide verifiable proof of authorization.

The main challenge with hashes is that the entirety of a piece of content
needs to be acquired in order to validate the hash.
This document describes a method of hash generation
that can be verified progressively.
This enables systems that depend on progressive processing,
such as streaming processing of a long sequence of records,
or performance-sensitive applications like HTML rendering.

The way this works is by splitting a document into chunks,
or chiclets,
each comprising
a length,
some number of bytes of content, and
a hash for the next chiclet.
This is shown in {{f-overview}}.

~~~ aasvg
       <---.            .--------------.            .---- ...
            |          |                |          |
           H()         |               H()         |
           / \         |               / \         |
 .--------'   '--------)---. .--------'   '--------)---.
|                      v    |                      v    |
+-----+----~~~~----+--------+-----+----~~~~----+--------+
| Len |  Content   |  Hash  | Len |  Content   |  Hash  | ...
+-----+----~~~~----+--------+-----+----~~~~----+--------+

<-- First byte                                 Last byte -->
~~~
{: #f-overview title="Overview of Format"}

The hash of the entirety of the content
can thereby be represented by the hash of the first chiclet.

This enables progressive processing of content,
providing strong integrity for all content bytes,
provided that each chiclet is only released after being validated;
see {{consumption}}.
That can reduce latency - or perceived latency -
for applications that are sensitive to delays.

The cost of this scheme is that there is a small amount of overhead.
Additional bytes for the hash and length fields
are interspersed with content bytes.
The relative cost of that overhead depends on the size of chiclets;
an average chunk size of 4k adds an 0.83% overhead.

Constructing content in this form
requires that the entirety of the content is available;
see {{construction}}.


# Conventions and Definitions

{::boilerplate bcp14-tagged}

This document uses the varint encoding
and presentation language
developed for QUIC {{!QUIC=RFC9000}}.


# Chained Hash Integrity Format {#format}

Any continuous stream of bytes can be turned into
a stream of variable-length chiclets.
This format allows for progressive validation of the integrity of content.


## Format Description

The format comprises zero or more non-final chiclets and a final chiclet
(which can be the first).
Content bytes are therefore interspersed with framing and integrity bytes,
increasing the size of the content.

A non-final chiclet comprises a non-zero length (for framing),
between 1 and 2<sup>61</sup>-1 bytes (inclusive) of content (see {{length}}),
and a 32 byte hash (that is, integrity bytes).

Lengths are encoded using the variable-length integer format
from {{Section 16 of QUIC}}.
The hash is the output of the SHA-256 algorithm {{!SHA=RFC6234}}
that has processed input of the chiclet that follows.

The final chiclet includes a zero-valued length field,
any number of bytes of content up to 2<sup>61</sup> bytes, and
no hash.
The final chiclet ends when content ends,
which means that this format can only be used
where there is a distinct end to the protected content.
That might mean the end of a file,
the end of a stream, or
the end of an HTTP message; see {{http}}.

The hash value that is computed from the very first chiclet
is not written to the output.
Instead, it needs to be carried outside the content;
see {{digest}} for one method of indicating this value.

The format is illustrated in {{f-layout}}.

~~~
Chic Content-Coding {
  Chiclet (i) ...,
  Final Chiclet (..),
}

Chiclet {
  Length (i),
  Content (8..),
  Hash (256),
}

Final Chiclet {
  Length (i) = 0,
  Content (..),
}
~~~
{: #f-layout title="Layout of Content Encoding"}

The format uses SHA-256 for integrity {{!SHA=RFC6234}};
see {{hash}} for details.


## Consumption and Validation {#consumption}

An entity that receives a stream of bytes in this format
will need a hash value for the first chiclet
in order to validate the content.
That value might be provided via some alternative communication channel;
one method for carrying this hash value in HTTP fields
is defined in {{digest}}.

Without the hash of the first chiclet,
it is not possible to validate the integrity of the content.
Content can still be processed by removing the length and hash bytes.

The process for progressively validating and releasing content bytes
starts with an expected hash value,
then proceeds as follows:

1. Initialize a new instance of SHA-256 {{!SHA=RFC6234}}.

1. Read a variable-length integer.
   Update the hash instance with each byte as it is read.
   Abort with an error if the stream ends before reading a complete value.

1. Read the indicated number of bytes, plus 32, from the stream.
   If the length value is 0, read to the end of the stream instead.
   Update the hash instance with each byte as it is read.
   Abort with an error if the stream ends before reading this amount.

1. Finalize the hash and generate hash output.
   If the hash output is not identical to the expected value,
   abort with an error.

1. Release the content bytes.

1. If the length value is non-zero,
   replace the expected hash value
   with the last 32 bytes read from the stream
   and return to step 1.
   Otherwise, end the process successfully.

This process ensures that released bytes are all integrity-protected,
according to the value that was provided.

To account for variations in how chiclets are constructed,
multiple hash values might be provided.
An entity that wishes to validate against any value
from a set of provided hash values
matches the hash of the first chiclet against the provided set.
Once the first chiclet is processed,
subsequent chiclets are validated against the single hash
from the preceding chiclet.


## Construction

In order to produce hashes,
content needs to be processed in reverse order.
Content is split into multiple pieces,
which are turned into chiclets.
These chiclets are hashed,
starting from the last,
placing the resulting hash
from each chiclet
at the end of the immediately preceding chiclet.

When constructing content,
if the content is processed in a forward direction,
chiclets might be written with placeholders values for hashes.
Hashes cannot be added until all subsequent chiclets -
and their hashes -
are computed.

The construction of chiclets can operate on fixed-sized pieces,
or application needs can drive how content is split.
For instance, an image that can be displayed
at progressively-higher levels of quality can have certain points at which
significant quality improvements can be realized;
aligning chiclets to those points might enable better incremental processing.

The fact that content needs to be processed in reverse order
makes this format ill-suited to situations
where content of arbitrary length is streamed.
It is also poorly suited to use with dynamically-generated content,
especially where content is latency sensitive,
because it requires two processing passes to produce the format.
It is therefore best suited to static resources
where the format can be pregenerated and saved.


## Length Constraints {#length}

An implementation that processes input needs to hold an entire chiclet
in its memory before releasing it.
Implementations that accept untrusted inputs
might refuse to support large chiclets
to protect against memory exhaustion attacks.

Implementations of this specification MUST support chiclets
with length values of up to 16383 bytes (2<sup>14</sup>-1).
This corresponds to a total chiclet length of 16417.
Larger chiclets can be used with prior arrangement.

The absolute maximum length of a chiclet is determined by the length encoding in SHA-2,
which is expressed in bits as a 64-bit value.
Consequently, the maximum length is 2<sup>61</sup>-1 bytes,
not the 2<sup>62</sup>-1 bytes of content that can be expressed
in a QUIC variable-length integer.
For a chiclet of that size, the length and hash further reduce
the available content length
by an additional 40 bytes to 2<sup>61</sup>-41;
the final chiclet might reduce that by as little as one byte
to 2<sup>61</sup>-2 bytes of content.
Of course, there is little value in having such large chiclets,
which directly act against the goal of incremental processing.

An abusive implementation might also seek to generate wasteful work
on receipients
by constructing many small chiclets.
While small chiclets might be necessary for some applications,
very small lengths can add significant amounts of overhead.
Recipients therefore need to look for abusive patterns,
such as receiving a large number of chiclets below a given threshold,
rather than have a strict lower bound on chiclet length.


# HTTP Usage {#http}

The use of progressive validation
is well-suited to HTTP applications {{!HTTP=RFC9110}} that rely
on the streaming and incremental processing of message content.

A content-coding is defined to describe content that can be progressively decoded;
see {{coding}}.
A digest format is defined to allow the hash of the first chiclet
to be signaled;
see {{digest}}.


## Content-Coding {#coding}

The `chi` content coding indicates that the content of the request
is a sequence of chiclets,
following the construction defined in {{format}}.


## Digest Format {#digest}

The `chi` hash algorithm label
identifies the use of the format described in {{format}}
when carried in the Content-Digest or Repr-Digest fields
that are defined in {{!DIGEST=RFC9530}}.

The `chi` digest format is only useful in combination
with a very specific `chi` content-coding; see {{coding}}.
Any change to the length of chiclets in that coding
will invalidate the value of the digest format.
This also means that use with `Repr-Digest` might not be useful
if the length of chiclets could change
between different encodings of the same representation.

The value associated the `chi` hash algorithm is a 32-byte binary value,
represented as a structured field Byte Sequence;
see {{Sections 2 and 3 of DIGEST}} for details.

Unlike other hash algorithms,
the `chi` hash algorithm value can only be used
during the removal of the `chi` content coding.
It therefore provides integrity for content,
but not necessarily the exact sequence of bytes that appear in the message.
For example, another content coding -
such as a compression coding -
could be applied after the `chi` coding.

Note that this format does not allow for multiple values
of the `chi` hash algorithm.
Only the last value is visible to a recipient;
see {{Section 4.2.2 of ?RFC9651}}.

## Example {#example}

TODO


# Security Considerations

The security properties of this feature is intended to be
broadly equivalent to simple hashing of content,
with the added condition that a recipient can use parts of content
prior to receiving every byte.

The integrity property depends on the hash function chosen ({{hash}}),
but exposes recipients to a risk of truncation ({{truncation}}).
There is also a resource exhaustion exposure,
particularly for recipients with limited resources ({{dos}}).


## Hash Choice {#hash}

This document relies on the second pre-image resistance property of SHA-256 {{!SHA=RFC6234}}.
If a recipient holds a SHA-256 hash for some content,
an attacker that wishes to substitute content
needs to a plaintext that will produce the same hash.

This document does not define the use of an alternative hash function.
In the event that attacks
on the second pre-image resistance of SHA-256 are found,
new labels can be defined in a new specification.
The definition of those labels could reuse the construction in this document
with a different hash function.


## Truncation {#truncation}

The use of a streaming format means that
content might be released to an application for processing,
with no guarantee that subsequent chiclets will be delivered.
Any application that uses this format
therefore needs to be robust in the event that content is truncated.

The final chiclet contains a zero-length prefix.
That prefix is hashed,
ensuring that the final chiclet is easily distinguished
from any non-final chiclets.
This guarantees that truncation can be detected.


## Chiclet Size {#dos}

The requirement to hold an entire chiclet until it is validated
creates a denial of service risk;
see {{length}} for discussion and mitigations.


# IANA Considerations

TODO: register both `chi` things.


--- back

# Acknowledgments
{:numbered="false"}

TODO acknowledge.
