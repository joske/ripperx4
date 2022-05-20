20 years ago I was maintainer of ripperX, which was written in C and gtk 1.x. I started a rewrite (also in C, but using gtk 2), called ripperX 3. The code was still forking cdparanoia and lame and reading its output... But it was never finished and abandoned (also by the new maintainers). My friend Kris created a library libcddb (which still exists, although also no longer maintained by him). I came across the ripperx3 branch (the source is still available on sourceforge after all those years!) and tried to revive it, but not much sense in reviving an app written in C and linked against an obsolete GTK+ version. So I started rewriting it in Rust, gstreamer and GTK 4. 

This is mostly a learning excercise for me, not sure if this will get released at all (but hey, it's on github so anyone can build it ;-)). I also created (and even published!) a crate to query CDDB server (which is now hosted by the GNU project at gnudb.org) called gnudb (see https://crates.io/crates/gnudb).

Right now, it starts up, can scan the CD drive, and rip/encode the tracks to MP3, OGG, or flac. There is only minimal error handling, so don't expect much. But when nothing goes wrong, it actually kinda sorta works!
