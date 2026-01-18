// Desktop platforms (x86, x86_64, aarch64)
#[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
use std::thread;
#[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
use std::time::Duration;
#[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
use time::OffsetDateTime;

// ESP32 platforms (xtensa, riscv32)
#[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
use esp_idf_hal::delay::FreeRtos;
#[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
use esp_idf_sys::xTaskGetTickCount;

#[inline]
pub fn tick_count() -> u32 {
    #[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
    {
        unsafe { xTaskGetTickCount() as u32 }
    }
    #[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
    {
        (OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as u32
    }
}

#[inline]
pub fn sleep(ms: u32) {
    #[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
    FreeRtos::delay_ms(ms);
    #[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
    thread::sleep(Duration::from_millis(ms as u64));
}
