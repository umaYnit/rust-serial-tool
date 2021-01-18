[rust-raspberrypi-OS-tutorials教程](https://github.com/rust-embedded/rust-raspberrypi-OS-tutorials)中ruby工具的Rust实现，便于在只有Rust的环境使用（例如win），使用方式同原工具：

```shell
cargo run --bin mini_term [serial_name]
cargo run --bin mini_push [serial_name] [image_path]

eg:
cargo run --bin mini_term COM3
cargo run --bin mini_term COM3 C:\image_rpi4.img
```

