pub struct ChipConf {
    pub lp_target : &'static str,
    pub lp_features : &'static str,
    pub lp_args : &'static str
}

pub fn get_conf_by_chip_name<S: AsRef<str>>(chip_name: S) -> Option<ChipConf> {
    // Implementation for retrieving chip configuration based on name
    match chip_name.as_ref().to_lowercase().as_str() {
        "esp32c6" | "esp32-c6" => Some(ChipConf {
            lp_target: "riscv32imac-unknown-none-elf",
            lp_features: "esp32c6",
            lp_args: ""
        }),
        "esp32c5" | "esp32-c5" => Some(ChipConf {
            lp_target: "riscv32imac-unknown-none-elf",
            lp_features: "esp32c5",
            lp_args: ""
        }),
        "esp32p4" | "esp32-p4" => Some(ChipConf {
            lp_target: "riscv32imac-unknown-none-elf",
            lp_features: "esp32p4",
            lp_args: ""
        }),
        "esp32s3" | "esp32-s3" => Some(ChipConf {
            lp_target: "riscv32imc-unknown-none-elf",
            lp_features: "esp32s3",
            lp_args: ""
        }),
        _ => None
    }
}