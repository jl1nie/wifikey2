// Desktop platforms (x86, x86_64, aarch64)
#[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
use std::thread;
#[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
use std::time::{Duration, Instant};

// ESP32 platforms (xtensa, riscv32)
#[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
use esp_idf_hal::delay::FreeRtos;

/// 起動からの経過ミリ秒を返す（単調クロック）
///
/// - ESP32: esp_timer_get_time() を使用（CONFIG_FREERTOS_HZに依存しない）
/// - Desktop: std::time::Instant を使用（NTP等による時刻逆行の影響なし）
#[inline]
pub fn tick_count() -> u32 {
    #[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
    {
        // esp_timer_get_time() は常にマイクロ秒を返す（FreeRTOSティックレートに非依存）
        unsafe { (esp_idf_sys::esp_timer_get_time() / 1000) as u32 }
    }
    #[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
    {
        use std::sync::OnceLock;
        static START: OnceLock<Instant> = OnceLock::new();
        let start = START.get_or_init(Instant::now);
        start.elapsed().as_millis() as u32
    }
}

#[inline]
pub fn sleep(ms: u32) {
    #[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
    FreeRtos::delay_ms(ms);
    #[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
    thread::sleep(Duration::from_millis(ms as u64));
}
