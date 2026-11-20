# Compressed JSON with Conan

This example serializes a JSON document with nlohmann/json, compresses it with
zlib, and uses GoogleTest to check the round trip and reject truncated input.
The executable and tests share the `zipjson` library under `src/libs/`.

With Conan 2, pkg-config, Clang, and a compatible default Conan profile, run
these commands from the repository root after building easypp:

```sh
cd examples/conan-json
../../target/debug/easypp deps install --release -- --build=missing
../../target/debug/easypp run --release
../../target/debug/easypp test --release
../../target/debug/easypp info --release
```

The pinned versions are recorded in [conanfile.txt](conanfile.txt). Conan builds
missing package binaries on the first install. Those dependency
recipes may require CMake. The example itself uses easypp with no project
build file. The `info` output marks the three dependencies as Conan packages
and lists GoogleTest only for the test target.
