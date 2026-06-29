# mtukai-projgen
A cargo and development tool to generate projects for mtukai-powered heterogeneous-ISA multicore architecture embedded softwares.
This tool allows easier use of [mtukai](https://github.com/puyogo-suzuki/mtukai) library.

# Target Devices
Currently this supports:
 - ESP32-S3
 - ESP32-C6

# Get Started
Before you start, please clone [mtukai](https://github.com/puyogo-suzuki/mtukai) on `./mtukai`, then see `examples/list_sum`.  

## Guide for mtukai-projgen
`Cargo.toml` contains a meta data for `cargo-mtukai`.  
```toml
[[package.metadata.mtukai.build]]
name = "default"
lp = {
    features = "esp32c6",
    target = "riscv32imac-unknown-none-elf"
}
main = {
    features = "esp32c6"
}
template = "esp32c6"

[[package.metadata.mtukai.build]]
name = "esp32s3"
chip = "esp32s3"
```

`name` is a build configuration name.
`default` is configured for ESP32-C6, and `esp32s3` is for ESP32-S3.
Currently, this example supports both ESP32-C6 and ESP32-S3.  
Look at `features` seciton.
There are four features: `esp32c6`, `esp32s3`, `has-lp-core`, and `is-lp-core`.
`esp32c6` and `esp32s3` are platform dependent features.
The build configuration enables these features following to `lp.features` or `main.features` definition.
`has-lp-core` and `is-lp-core` are reserved features.
`has-lp-core` is enabled on the main processor, but not on the LP coprocessor.
On the other hand, `is-lp-core` is only enabled on the LP coprocessor.  
Look at `dependencies` definition.
You must reference four crates: `esp-rs-copro`, `mtukai-projgen`, `esp-rs-copro-procmacro`, `mtukai-projgen-procmacro`.  

Now look at the source code ([`examples/list_sum/src/bin/main.rs`](examples/list_sum/src/bin/main.rs)).  
Currently, the functions for the LP coprocessor must be attributed with `#[cfg(feature = "is-lp-core")]`, and for the main processor, attributed with `#[cfg(feature = "has-lp-core")]`.  
To work with `mtukai`, the declaration for the allocator on the LP coprocessor is required:
```rust
#[cfg(feature = "has-lp-core")]
define_lp_allocator!();
```

The entry point of the LP coprocessor must be attributed with `entry` macro.
```rust
#[mtukai_projgen_procmacro::entry(4096)]
fn lpmain(_ : &mut LpContext, data : &mut LPBox<SimpleList>, to_add : i32, to_be_summed : &[i32], result : &mut i32) -> ! {
    ...
    wake_hp_core();
    lp_core_halt()
}
```
You cannot use the first argument.
You can pass parameters as arguments.
The types of arguments must implement `MovableObject`.
You can pass as a slice, reference, mutable reference, or `LPBox`.


How to call `lpmain`?
```rust
let mut lp_context = LpContext::new(&mut lp_core, &mut rtc);
if let Err(e) = lpmain(&mut lp_context, &mut data, TO_ADD, &to_be_summed, &mut result) {
    println!("Error running LP core: {}", e);
}
```
`LpContext` holds necessary peripherals (Technically, it is not required but the current design requires it.)  
The return value will be `Result<(), EspCoproError>`.  
Please return the result value via `&mut` arguments.

How to place the specified files for the specific processsors?
See [`examples/list_sum/template`](examples/list_sum/template/).
There are directries named same as `template` of the build configurations.
Open one of the directory, there are two directories: `main` and `lp`.
The files placed at `template/{template name}/{lp, main}` are placed for the projects for the specific processors.
If files on the same path are located on the top directory, these files are overwritten with ones on the `template` directory.

## How to compile?
### Project duplication
Currently, please run the following command on **[`./cargo-mtukai`](./cargo-mtukai/)**:
```sh
$ cargo run -- --manifest-path ../examples/list_sum/Cargo.toml gen
```
This generates processor-specific projects for ESP32-C6.  
For ESP32-S3, please run:
```sh
$ cargo run -- --manifest-path ../examples/list_sum/Cargo.toml --build_name esp32s3 gen
```
You can specify the build configration with the name.

### Build or Run
Currently, please run the following command on **[`./cargo-mtukai`](./cargo-mtukai/)**:
```sh
$ cargo run -- --manifest-path ../examples/list_sum/Cargo.toml run
```
This generates processor-specific projects for ESP32-C6.  
For ESP32-S3, please run:
```sh
$ cargo run -- --manifest-path ../examples/list_sum/Cargo.toml --build_name esp32s3 run
```
You can specify the build configration with the name.

If you want to only build, please use `build` subcommand:
```sh
$ cargo run -- --manifest-path ../examples/list_sum/Cargo.toml build
```

# Tools and Libraries
This repository consists of 1 tool and 2 crates (macro crate and library crate).  
A tool `cargo-mtukai` duplicates projects and compiles/runs the projects based on the configurations written in `Cargo.toml`.  
Traditional heterogeneous multicore emebedded systems development requires each own project for each processor.  
`mtukai` also requires two projects for main processor and LP coprocessor.  
`cargo-mtukai` allows to write once, compiles for two processors (with some drawbacks).  
For the entry point of LP coprocessor and the LP coprocessor execution, `mtukai-projgen` and `mtukai-projgen-macro` provides `entry` macro.

## Metadata in `Cargo.toml`
Please write metadata to `Cargo.toml`.  
`package.metadata.mtukai.build` defines build configurations.
A build configuration consists of `name`, `template`, build parameters for each processor.
`name` is an identifier for the command line argument `--build_name` or `-b`.
`template` is an identifier for the template directory.
If `template` is not specified, it will be same as `name`.
The build paramters `main` and `lp` are passed when building.
You can specify `features`, `raget`, and additional arguments `args`.
If you specify `chip`, these build parameters are automatically set.
Currently, `chip` must be either: `esp32s3` or `esp32c6`.

```toml
[[package.metadata.mtukai.build]]
name = "default"
lp = {
    features = "esp32c6",
    target = "riscv32imac-unknown-none-elf",
    args = ""
}
main = {
    features = "esp32c6"
}
template = "esp32c6"

[[package.metadata.mtukai.build]]
name = "esp32s3"
chip = "esp32s3"
```