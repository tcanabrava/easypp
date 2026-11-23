# C++ on ARM64 Linux with QEMU

This example builds a byte checksum library, a console program, and a test
using the C++ standard library. The checksum is the sum of the bytes modulo 256.

From this directory, build and run on your host:

```sh
easypp run
easypp test
```

With the [ARM64 toolchain and QEMU user-mode tools](../../README.md#arm64-linux-in-qemu),
cross-build and run the same program and test:

```sh
easypp run --target aarch64-linux-gnu
easypp test --target aarch64-linux-gnu
```

The ARM64 executable prints `ARM64 checksum: 42`. The test covers an empty
input, byte addition, and wraparound.
