#![no_std]
#[cfg(feature = "is-lp-core")]
pub struct LpContext<'ctx, 'core, 'rtc> { 
    phantom_ctx: core::marker::PhantomData<&'ctx ()>,
    phantom_core: core::marker::PhantomData<&'core ()>,
    phantom_rtc: core::marker::PhantomData<&'rtc ()>
}

#[cfg(feature = "has-lp-core")]
pub struct LpContext<'ctx, 'core, 'rtc> {
    #[cfg(feature = "esp32c6")]
    lp_core: &'core mut esp_hal::lp_core::LpCore<'core>,
    #[cfg(feature = "esp32s3")]
    lp_core: &'core mut esp_hal::ulp_core::UlpCore<'core>,
    #[cfg(any(feature = "esp32c6", feature = "esp32s3"))]
    rtc : &'rtc mut esp_hal::rtc_cntl::Rtc<'rtc>,
    phantom_ctx: core::marker::PhantomData<&'ctx ()>,
}

#[cfg(any(feature = "is-lp-core", feature = "has-lp-core"))]
impl<'ctx, 'core, 'rtc> LpContext<'ctx, 'core, 'rtc> {
    #[cfg(all(feature = "has-lp-core", feature = "esp32c6"))]
    pub fn new(lp_core: &'core mut esp_hal::lp_core::LpCore<'core>, rtc: &'rtc mut esp_hal::rtc_cntl::Rtc<'rtc>) -> LpContext<'ctx, 'core, 'rtc> {
        LpContext { lp_core, rtc, phantom_ctx: core::marker::PhantomData }
    }

    #[cfg(feature = "is-lp-core")]
    pub fn new() -> LpContext<'ctx, 'core, 'rtc> {
        LpContext { phantom_ctx: core::marker::PhantomData, phantom_core: core::marker::PhantomData, phantom_rtc: core::marker::PhantomData }
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
    pub fn get_rtc(&mut self) -> &mut esp_hal::rtc_cntl::Rtc<'rtc> {
        self.rtc
    }
}