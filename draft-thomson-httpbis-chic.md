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
area: "ART"
workgroup: "HTTP"
keyword:
 - blockchain
venue:
  group: "HTTP"
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
  SRI:
    title: "Subresource Integrity"
    date: 2026-03-20
    author:
      - name: Frederik Braun
    target: "https://w3c.github.io/webappsec-subresource-integrity/"


--- abstract

This document defines a content-coding for HTTP that provides strong integrity protection
while maintaining the ability to perform some progressive processing.


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
That can reduce latency -- or at least perceived latency --
for applications that are sensitive to delays.

The cost of this scheme is that there is a small amount of overhead.
Additional bytes for the hash and length fields
are interspersed with content bytes.
The relative cost of that overhead depends on the size of chiclets;
an average size of 4KiB adds overhead of 0.83%.

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
between 1 and 2<sup>61</sup>-41 bytes (inclusive) of content (see {{length}}),
and a 32 byte hash (that is, integrity bytes).

Lengths are encoded using the variable-length integer format
from {{Section 16 of QUIC}}.
Lengths do not need to be encoded in the minimal form;
the chosen encoding will affect the resulting digest.

The hash is the output of the SHA-256 algorithm {{!SHA2=RFC6234}}
that covers the entirety of the subsequent chiclet.

The final chiclet includes a zero-valued length field,
up to 2<sup>61</sup>-2 bytes of content
(less if the length is encoded in more than one byte),
and no hash value.
The final chiclet ends when content ends,
which means that this format can only be used
where there is a distinct end to the protected content.
That might mean the end of a file,
the end of a stream, or
the end of an HTTP message; see {{http}}.

The hash value that is computed from the very first chiclet
is not written to the output.
This is the value that commits to the entire sequence of chiclets
and the content that it protects.
This value needs to be carried outside the content;
see {{digest}} for one method of indicating this value.

The format is illustrated in {{f-layout}}.

~~~
Chic Content-Coding {
  Chiclet (..) ...,
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

The format uses SHA-256 for integrity {{!SHA2}};
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

1. Initialize a new instance of SHA-256 {{!SHA2}}.

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
Though this is not possible with the Repr-Digest and Content-Digest fields
(see {{digest}}),
digest values might be provided via other means,
such as a future extension to {{SRI}}.
An entity that wishes to validate against any value
from a set of provided hash values
matches the hash of the first chiclet against the provided set.
Once the first chiclet is processed,
subsequent chiclets are validated against the single hash
from the preceding chiclet.


## Construction {#construction}

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
chiclets might be written with placeholder values for hashes.
Hashes cannot be added until all subsequent chiclets --
and their hashes --
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

Receivers MUST support chiclets
with length values of up to 16383 bytes (2<sup>14</sup>-1).
This corresponds to a complete chiclet having a maximum length
of 16423 for non-final chiclets
and 16391 for a final chiclet.
Encoders MUST NOT generate larger chiclets
unless support for larger values has been arranged.

A receiver can validate that the length of a non-final chiclet
is acceptable before reading further;
the final chiclet does not have a length prefix,
so receivers need to either rely on other length indicators
(such as the HTTP Content-Length field)
or abort when received bytes exceed the configured maximum.

The absolute maximum length of a chiclet is determined by the length encoding in SHA-2,
which is expressed in bits as a 64-bit value.
Consequently, the maximum length is 2<sup>61</sup>-1 bytes,
less overheads.
This is less than the 2<sup>62</sup>-1 length that could otherwise
be expressed in a QUIC variable-length integer.
For a chiclet of that size, the length and hash further reduce
the available content length
by an additional 40 bytes to 2<sup>61</sup>-41;
the final chiclet carries only a zero-valued length,
which could be one, two, four, or eight bytes long.
Of course, there is little value in having such large chiclets,
which directly act against the goal of incremental processing.

An abusive implementation might also seek to generate wasteful work
on recipients
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

The `chi` content coding indicates that the content of the message
is a sequence of chiclets,
following the construction defined in {{format}}.
Encoding follows the guidance in {{construction}};
decoding follows the procedure in {{consumption}}.

A decoder can read a message that uses `chi` content coding
without a target digest,
though that provides no integrity guarantee.

A client can indicate that it will accept `chi` content coding
in responses using the Accept-Encoding field.
A client can attempt to use `chi` content coding on requests,
which could result in an error response that carries Accept-Encoding
if it is not understood by the server;
see {{Section 12.5.3 of HTTP}} for details.

Intermediaries can check, add, or remove `chi` content coding
at their discretion.
Adding the coding requires buffering of much of the message
before the hash for any non-final chiclet can be forward;
see {{construction}}.
Removal invalidates any digest field; see {{digest}}.

The `chi` content coding cannot be validated if used with a range request
({{Section 14 of HTTP}}).


## Digest Format {#digest}

The `chi` hash algorithm label
identifies the use of the format described in {{format}}
when carried in the Content-Digest or Repr-Digest fields
that are defined in {{!DIGEST=RFC9530}}.

The `chi` digest format is only useful in combination
with a very specific `chi` content-coding; see {{coding}}.
Any change to the length of chiclets in that coding
will invalidate the value of the digest format.
This also means that use with Repr-Digest might not be useful
if the length of chiclets could change
between different encodings of the same representation.

Values in Content-Digest and Repr-Digest
apply to the message after the application of all content codings.
This means that a `chi` digest that is included in those fields
is only useful if the `chi` content coding is the last coding applied.
Other ways of conveying the digest do not have this limitation.

Note also that only the last digest value in these fields is visible
to a recipient;
see {{Section 4.2.2 of ?RFC9651}}.

The value associated with the `chi` hash algorithm is a 32-byte binary value,
represented as a structured field Byte Sequence;
see {{Sections 2 and 3 of DIGEST}} for details.

Unlike other hash algorithms,
the `chi` hash algorithm value can only be used
during the removal of the `chi` content coding.
Decoding removes the coding
and provides strong integrity based on the digest value.


## Example {#example}

This example shows content of 47 bytes, split into chiclets of 16 bytes.
This chiclet size is far smaller than would be used in practice,
but it allows the example to show multiple chiclets.
The resulting 3 chiclets are shown in {{ex-chiclets}},
with the hash of each, starting from the last.

~~~
Chiclet 3:
  Length: 0
  Content: " the next one.\n"
  SHA-256 of Chiclet 3:
    1ea4150d 2454faec 9e8b2993 aac8ef95
    15477606 7c59005c 57f00583 81018b09

Chiclet 2:
  Length: 16
  Content: "ries the hash of"
  Hash of Chiclet 3:
    1ea4150d 2454faec 9e8b2993 aac8ef95
    15477606 7c59005c 57f00583 81018b09
  SHA-256 of Chiclet 2:
    c7156912 ad30c2a3 910cc3d8 06ed2318
    aa25474f ff11f6a8 9efb6e6d 4ff7fe84

Chiclet 1:
  Length: 16
  Content: "Each chiclet car"
  Hash of Chiclet 2:
    c7156912 ad30c2a3 910cc3d8 06ed2318
    aa25474f ff11f6a8 9efb6e6d 4ff7fe84
  SHA-256 of Chiclet 1:
    e3bb01bc b8baa8d0 9010fb28 66b73d27
    d8ff408a b74a7bc7 2af12be4 464f94d7
~~~
{: #ex-chiclets title="Example Chiclets"}

The hash of the first chiclet can be carried in the Content-Digest field,
as shown in the HTTP response in {{ex-response}}.

~~~ http-message
HTTP/1.1 200 OK
Content-Type: text/plain
Content-Encoding: chi
Content-Length: 114
Content-Digest: chi=:47sBvLi6qNCQEPsoZrc9J9j/QIq3SnvHKvEr5EZPlNc=:
~~~
{: #ex-response title="Example Response Header"}

The 114 bytes of response content are shown in {{ex-content}} in hexadecimal.

~~~
10                                    # Length = 16
45616368 20636869 636c6574 20636172   # "Each chiclet car"
c7156912 ad30c2a3 910cc3d8 06ed2318   # Hash of Chiclet 2
aa25474f ff11f6a8 9efb6e6d 4ff7fe84
10                                    # Length = 16
72696573 20746865 20686173 68206f66   # "ries the hash of"
1ea4150d 2454faec 9e8b2993 aac8ef95   # Hash of Chiclet 3
15477606 7c59005c 57f00583 81018b09
00                                    # Length = 0
20746865 206e6578 74206f6e 652e0a     # " the next one.\n"
~~~
{: #ex-content title="Example Encoded Content"}

Empty content is encoded as a single final chiclet,
comprising a single byte with a value of 0.
The `chi` digest of empty content is therefore the SHA-256 hash
of that one byte, as shown in {{ex-empty}}.

~~~ http-message
Content-Encoding: chi
Content-Length: 1
Content-Digest: chi=:bjQLnP+zepicpUTmu3gKLHiQHT+zNzh2hRGjBhevoB0=:
~~~
{: #ex-empty title="Fields for Empty Content"}

Note that these examples use the minimal encoding
for the variable-length integers.
Longer encodings for those integers would produce different digest values.


# Security Considerations

This coding provides integrity protection equivalent
to hashing the message content.
However, there are subtle differences that need to be noted.

The integrity property depends on the hash function chosen ({{hash}}).
Recipients are exposed to a risk of message truncation ({{truncation}}).
There is also a resource exhaustion exposure,
particularly for recipients with limited resources ({{dos}}).
Finally, the digest value produced is not canonical ({{non-canon}}).


## Hash Choice {#hash}

This document relies on the second pre-image resistance property of SHA-256 {{!SHA2}}.
If a recipient holds a SHA-256 hash for some content,
an attacker that wishes to substitute content
needs to find a second plaintext that will produce the same hash.

If the digest is subsequently signed,
the collision resistance of SHA-256 is also relied on
to prevent message substitution.

This document does not define the use of an alternative hash function.
In the event that attacks on SHA-256 are found,
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
The prefix of every chiclet is hashed,
but the hashing of a zero length ensures
that the final chiclet is easily distinguished from any non-final chiclets.


## Chiclet Size {#dos}

The requirement to hold an entire chiclet until it is validated
creates a denial of service risk;
see {{length}} for discussion and mitigations.


## Non-Canonical {#non-canon}

The same sequence of bytes can produce many different integrity values.
As a result, the hash of the first chiclet cannot act
as a canonical identifier for content.
Each digest value commits to a specific sequence of chiclets,
so cannot be used with a different arrangement of chiclet lengths.


# IANA Considerations

This document registers a content coding and a hash algorithm for HTTP digest fields.


## Content Coding

IANA is requested to add the following entry
to the ["HTTP Content Coding Registry"](https://www.iana.org/assignments/http-parameters/#content-coding)
established by {{Section 16.6.1 of HTTP}}:

Name:
: chi

Description:
: Chained Hash Integrity Coding

Reference:
: {{coding}} of this document


## Digest Hash Algorithm

IANA is requested to add the following entry
to the ["Hash Algorithms for HTTP Digest Fields" registry](https://www.iana.org/assignments/http-digest-hash-alg/)
established by {{Section 7.2 of DIGEST}}:

Algorithm Key:
: chi

Status:
: Active

Description:
: The SHA-256 hash of the first chiclet of the `chi` content coding

Reference:
: {{digest}} of this document


--- back

# Acknowledgments
{:numbered="false"}

{{{Jeffrey Yasskin}}} was a co-author on a previous iteration of this work.
