#[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
use esp_idf_hal::delay::FreeRtos;
#[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
use esp_idf_sys::xTaskGetTickCount;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use time::OffsetDateTime;

#[inline]
pub fn tick_count() -> u32 {
    #[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
    {
        unsafe { xTaskGetTickCount() as u32 }
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        (OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as u32
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub async fn sleep(ms: u32) {
    tokio::time::sleep(tokio::time::Duration::from_millis(ms as u64)).await;
}

#[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
#[inline]
pub fn sleep(ms: u32) {
    FreeRtos::delay_ms(ms);
}
