# RipperX 4

[![workflow status](https://github.com/joske/ripperx4/actions/workflows/rust.yml/badge.svg)](https://github.com/joske/ripperx4/actions?query=workflow%3A%22rust%22)
[![security audit](https://github.com/joske/ripperx4/actions/workflows/audit.yml/badge.svg)](https://github.com/joske/ripperx4/actions?query=workflow%3A%22Security+Audit%22)

20 years ago I was maintainer of ripperX, which was written in C and gtk 1.x. I
started a rewrite (also in C, but using gtk 2), called ripperX 3. The code was
still forking cdparanoia and lame and reading its output... But it was never
finished and abandoned (also by the new maintainers). My friend Kris created a
library libcddb (which still exists, although also no longer maintained by
him). I came across the ripperx3 branch (the source is still available on
sourceforge after all those years!) and tried to revive it, but not much sense
in reviving an app written in C and linked against an obsolete GTK+ version. So
I started rewriting it in Rust, gstreamer and GTK 4.

This is mostly a learning excercise for me, not sure if this will get released
at all (but hey, it's on github so anyone can build it ;-)). It includes code
to query the disc on musicbrainz service (previous versions used gnudb, but
that service seems down now, so I bit the bullet and implemented a basic query
to musicbrainz vast info).

It is now almost feature complete, see below:

## What works

- can scan CDROM drive
- query musicbrainz with fallback to gnudb
- you can edit the data
- adds tags to the files
- you can select which tracks to rip
- supports MP3, OGG, FLAC, OPUS and WAV
- you can set quality options
- sends notification when done
- doesn't overwrite existing files without warning

## What is not supported (yet)

- no support for multiple matches from musicbrainz (just takes the first match)
- composer field

## Building

Install gtk 4, gstreamer, libdiscid

`cargo build`

Tip: builds for macOS and linux are available on every build in Actions/Artifacts.

## Running

`cargo run`
