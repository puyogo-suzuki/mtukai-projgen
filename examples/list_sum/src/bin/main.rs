#![no_std]
#![no_main]
#![cfg_attr(feature = "is-lp-core", feature(alloc_error_handler))]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]


#[cfg(feature = "has-lp-core")]
use {
    esp_alloc as _,
    esp_hal::rtc_cntl::Rtc,
    esp_hal::lp_core::{LpCore, LpCoreWakeupSource},
    esp_rs_copro_procmacro::{define_lp_allocator, load_lp_code2},
    esp_println::{print, println}
};

use esp_rs_copro::{lpbox::LPBox, prelude::*};
use core::option::Option;

#[cfg(feature = "is-lp-core")]
use {
    esp_lp_hal::{prelude::entry, delay::Delay},
    esp_rs_copro::prelude::*,
    panic_halt as _
};


#[cfg(feature = "has-lp-core")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[cfg(feature = "is-lp-core")]
esp_rs_copro_procmacro::esp_rs_copro_statics!(4096);
#[cfg(feature = "is-lp-core")]
#[alloc_error_handler]
fn ignore_alloc_error(_: core::alloc::Layout) -> ! {
    loop{}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
#[cfg(feature = "has-lp-core")]
esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(feature = "has-lp-core")]
define_lp_allocator!();

#[derive(esp_rs_copro_procmacro::MovableObject)]
pub struct SimpleList {
    pub value : i32,
    pub next : Option<LPBox<SimpleList>>
}

impl SimpleList {
    pub fn new(value : i32, next : Option<LPBox<SimpleList>>) -> Self {
        SimpleList { value, next }
    }
    pub fn push(&mut self, value : i32) {
        match &mut self.next {
            Some(next) => next.push(value),
            None => self.next = Some(LPBox::new(SimpleList::new(value, None)))
        }
    }
    pub fn sum(&self) -> i32 {
        fn go(list : &SimpleList, res : i32) -> i32 {
            match &list.next {
                Some(next) => go(next, res + list.value),
                None => res + list.value
            }
        }
        go(self, 0)
    }
}

#[derive(esp_rs_copro_procmacro::MovableObject)]
pub struct MainLPParcel{
    pub data : LPBox<SimpleList>,
    pub result : i32
}

#[cfg(feature = "is-lp-core")]
#[entry]
fn main() -> !{
    let v: &mut MainLPParcel = get_transfer::<MainLPParcel>().unwrap();
    v.result = v.data.sum();
    v.data.push(10000);
    Delay.delay_millis(1000);
    wake_hp_core();
    lp_core_halt()
}

#[cfg(feature = "has-lp-core")]
fn gen_list() -> (SimpleList, i32) {
    fn go(val : i32) -> (SimpleList, i32) {
        let mut sl = SimpleList::new(1, None);
        for i in 2..val {
            sl = SimpleList::new(i, Some(LPBox::new(sl)));
        }
        let sum = sl.sum();
        (sl, sum)
    }
    go(10)
}

#[cfg(feature = "has-lp-core")]
fn print_list(list : &SimpleList) {
    print!("list: ");
    let mut current = list;
    loop {
        print!("{} ", current.value);
        match &current.next {
            Some(next) => current = next,
            None => break
        }
    }
    println!();
}

#[cfg(feature = "has-lp-core")]
#[esp_hal::main]
fn main() -> ! {
    // generator version: 0.5.0
    esp_alloc::heap_allocator!(size: 72 * 1024);
    esp_println::logger::init_logger_from_env();
    let delay = esp_hal::delay::Delay::new();
    let peripherals = esp_hal::init(esp_hal::Config::default());
    
    let mut lp_core = LpCore::new(peripherals.LP_CORE);
    lp_core.stop();
    println!("lp core stopped");

    // load code to LP core
    let lp_core_code = load_lp_code2!(
        "CANNOT BE COMPILED"
    );
    {
        let (list, expected_sum) = gen_list();
        print_list(&list);
        let mut parcel = MainLPParcel {
            data : LPBox::new(list),
            result : 0
        };
        println!("lpcore run");
        delay.delay_millis(1000); // FOR ESP32-S3 because the UART stuck after the HP core wake up without the delay.
        if let Err(e) = lp_core_code.run_light_sleep(&mut lp_core, LpCoreWakeupSource::HpCpu, &mut Rtc::new(peripherals.LPWR), &mut parcel) {
            println!("Error running LP core: {}", e);
        }
        println!("result: {} (expected: {})", parcel.result, expected_sum);
        print_list(&parcel.data);
        println!("result: {} (expected: {})", parcel.data.sum(), expected_sum + 10000)
    }
    loop {}
}