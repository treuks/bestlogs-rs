# bestlogs-rs

This is a reimplementation of [best-logs](https://github.com/ZonianMidian/best-logs) in Rust for fun and as a way for me to learn more Rust.

# How to build

```sh
cargo build --profile release-debug
```

I am preeeetty sure you might also currently need a system package for jemalloc and a devel package for it, and it might also only work on Linux, but after I'll get to a soft finish of the project and it'll have a proper full 1.0.0 release, I might switch to mimalloc, which will solve both of those problems.  

# Configuration
Included in this repository, is the `example_config.json` file, which should be filled in with a bunch of stock values. 

To deploy the project you need to copy that file and save it as `config.json`.

> [!IMPORTANT]
> Unlike the original project, when you run bestlogs-rs, it will not merge the contents of `config.json` with the example config. 
> This means that only the contents of `config.json` will be read and used. That is caused by the fact that I am lazy and I don't care.
