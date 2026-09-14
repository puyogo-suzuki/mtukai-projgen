#![no_std]
#[cfg(feature = "is-lp-core")]
pub struct LpContext<'ctx, 'core, 'lpwr> { 
    phantom_ctx: core::marker::PhantomData<&'ctx ()>,
    phantom_core: core::marker::PhantomData<&'core ()>,
    phantom_lpwr: core::marker::PhantomData<&'lpwr ()>
}

#[cfg(feature = "has-lp-core")]
pub struct LpContext<'ctx, 'core, 'lpwr> {
    #[cfg(feature = "esp32c6")]
    lp_core: &'core mut esp_hal::lp_core::LpCore<'core>,
    #[cfg(feature = "esp32s3")]
    lp_core: &'core mut esp_hal::ulp_core::UlpCore<'core>,
    #[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
    lpwr : &'lpwr mut esp_hal::rtc_cntl::sleep::LowPower<'lpwr>,
    phantom_ctx: core::marker::PhantomData<&'ctx ()>,
}

#[cfg(any(feature = "is-lp-core", feature = "has-lp-core"))]
impl<'ctx, 'core, 'lpwr> LpContext<'ctx, 'core, 'lpwr> {
    #[cfg(all(feature = "has-lp-core", feature = "esp32c6"))]
    pub fn new(lp_core: &'core mut esp_hal::lp_core::LpCore<'core>, lpwr: &'lpwr mut esp_hal::rtc_cntl::sleep::LowPower<'lpwr>) -> LpContext<'ctx, 'core, 'lpwr> {
        LpContext { lp_core, lpwr, phantom_ctx: core::marker::PhantomData }
    }
    #[cfg(all(feature = "has-lp-core", feature = "esp32s3"))]
    pub fn new(lp_core: &'core mut esp_hal::ulp_core::UlpCore<'core>, lpwr: &'lpwr mut esp_hal::rtc_cntl::sleep::LowPower<'lpwr>) -> LpContext<'ctx, 'core, 'lpwr> {
        LpContext { lp_core, lpwr, phantom_ctx: core::marker::PhantomData }
    }

    #[cfg(feature = "is-lp-core")]
    pub fn new() -> LpContext<'ctx, 'core, 'lpwr> {
        LpContext { phantom_ctx: core::marker::PhantomData, phantom_core: core::marker::PhantomData, phantom_lpwr: core::marker::PhantomData }
    }
    
    #[cfg(all(feature = "has-lp-core", feature = "esp32c6"))]
    pub fn get_core(&mut self) -> &mut esp_hal::lp_core::LpCore<'core> {
        self.lp_core
    }

    #[cfg(all(feature = "has-lp-core", feature = "esp32s3"))]
    pub fn get_core(&mut self) -> &mut esp_hal::ulp_core::UlpCore<'core> {
        self.lp_core
    }

    #[cfg(all(feature = "has-lp-core", any(feature = "esp32c6", feature = "esp32s3")))]
    pub fn get_lpwr(&mut self) -> &mut esp_hal::rtc_cntl::sleep::LowPower<'lpwr> {
        self.lpwr
    }
}