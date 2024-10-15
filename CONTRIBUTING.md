# Contributing

## Protocol Buffers

The `build.rs` script in this library depends upon the
[Protocol Buffers compiler][protoc]. Be sure that `protoc` is installed and
available within your `PATH`.

[protoc]: https://docs.rs/prost-build/latest/prost_build/#sourcing-protoc

## Python Dependencies

This repository uses the [`prometheus-client`][client-python] Python client
library in its test suite.

You may create and activate a virtual environment with this dependency
installed by running the following shell commands from the root of this
repository:

```shell
python -m venv ./venv
source venv/bin/activate
pip install prometheus-client
```

[client-python]: https://github.com/prometheus/client_python
