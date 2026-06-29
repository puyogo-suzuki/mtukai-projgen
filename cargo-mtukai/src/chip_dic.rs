pub struct ChipConfParams {
    pub target : &'static str,
    pub features : &'static str,
    pub args : &'static str,
}

pub struct ChipConf {
    pub main : ChipConfParams,
    pub lp : ChipConfParams,
    pub template : &'static str,
}

pub fn get_conf_by_chip_name<S: AsRef<str>>(chip_name: S) -> Option<ChipConf> {
    // Implementation for retrieving chip configuration based on name
    match chip_name.as_ref().to_lowercase().as_str() {
        "esp32c6" | "esp32-c6" => Some(ChipConf {
            main: ChipConfParams {
                target: "riscv32imac-unknown-none-elf",
                features: "esp32c6",
                args: "",
            },
            lp: ChipConfParams {
                target: "riscv32imac-unknown-none-elf",
                features: "esp32c6",
                args: "",
            },
            template: "esp32c6"
        }),
        "esp32c5" | "esp32-c5" => Some(ChipConf {
            main: ChipConfParams {
                target: "riscv32imac-unknown-none-elf",
                features: "esp32c5",
                args: "",
            },
            lp: ChipConfParams {
                target: "riscv32imac-unknown-none-elf",
                features: "esp32c5",
                args: "",
            },
            template: "esp32c5"
        }),
        "esp32p4" | "esp32-p4" => Some(ChipConf {
            main: ChipConfParams {
                target: "riscv32imac-unknown-none-elf",
                features: "esp32p4",
                args: "",
            },
            lp: ChipConfParams {
                target: "riscv32imac-unknown-none-elf",
                features: "esp32p4",
                args: "",
            },
            template: "esp32p4"
        }),
        "esp32s3" | "esp32-s3" => Some(ChipConf {
            main: ChipConfParams {
                target: "xtensa-esp32s3-none-elf",
                features: "esp32s3",
                args: "",
            },
            lp: ChipConfParams {
                target: "riscv32imc-unknown-none-elf",
                features: "esp32s3",
                args: "",
            },
            template: "esp32s3"
        }),
        _ => None
    }
}