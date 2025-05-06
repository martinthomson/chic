---
###
# Internet-Draft Markdown Template
#
# Rename this file from draft-todo-yourname-protocol.md to get started.
# Draft name format is "draft-<yourname>-<workgroup>-<name>.md".
#
# For initial setup, you only need to edit the first block of fields.
# Only "title" needs to be changed; delete "abbrev" if your title is short.
# Any other content can be edited, but be careful not to introduce errors.
# Some fields will be set automatically during setup if they are unchanged.
#
# Don't include "-00" or "-latest" in the filename.
# Labels in the form draft-<yourname>-<workgroup>-<name>-latest are used by
# the tools to refer to the current version; see "docname" for example.
#
# This template uses kramdown-rfc: https://github.com/cabo/kramdown-rfc
# You can replace the entire file if you prefer a different format.
# Change the file extension to match the format (.xml for XML, etc...)
#
###
title: "Chained Hash Integrity Coding for HTTP"
abbrev: "CHIC"
category: std

docname: draft-todo-yourname-protocol-latest
submissiontype: IETF  # also: "independent", "editorial", "IAB", or "IRTF"
number:
date:
consensus: true
v: 3
area: "Web and Internet Transport"
workgroup: "HTTP"
keyword:
 - next generation
 - unicorn
 - sparkling distributed ledger
venue:
  group: "HTTP"
  type: "Working Group"
  mail: "ietf-http-wg@w3.org"
  arch: "https://lists.w3.org/Archives/Public/ietf-http-wg/"
  github: "martinthomson/chic"
  latest: "https://martinthomson.github.io/chic/draft-thomson-httpbis-chic.html"

author:
 -
    fullname: Your Name Here
    organization: Mozilla
    email: your.email@example.com

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

~~~
TODO
~~~

The cost of this scheme is that there is a small amount of overhead
that is interspersed with the content bytes.


# Conventions and Definitions

{::boilerplate bcp14-tagged}

This document uses the varint encoding and presentation language developed for QUIC {{!QUIC=RFC9000}}.

# Length Constraints


# Security Considerations

TODO Security


# IANA Considerations

This document has no IANA actions.


--- back

# Acknowledgments
{:numbered="false"}

TODO acknowledge.
