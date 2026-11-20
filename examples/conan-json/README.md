# Compressed JSON with Conan

This example serializes a JSON document with nlohmann/json, compresses it with
zlib, and uses GoogleTest to check the round trip and reject truncated input.
The executable and tests share the `zipjson` library under `src/libs/`.

Easypp uses Conan 2 on PATH or installs it for your user with pipx or uv.
If there is no default Conan profile, easypp detects a local Clang profile.
With pkg-config and Clang available, run
these commands from the repository root after building easypp:

```sh
cd examples/conan-json
../../target/debug/easypp run --release
../../target/debug/easypp test --release
../../target/debug/easypp info --release
```

The pinned versions are recorded in [conanfile.txt](conanfile.txt). The first run installs those packages and
builds any missing binaries through Conan. Those dependency
recipes may require CMake. The example itself uses easypp with no project
build file. The `info` output marks the three dependencies as Conan packages
and lists GoogleTest only for the test target.
