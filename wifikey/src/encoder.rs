//! ロータリーエンコーダー クアドラチャデコーダー
//! WROVER32の4エンコーダー対応 (MAIN/SUB/MODE/BAND)

/// クアドラチャ状態テーブル: (prev_AB << 2 | curr_AB) → direction
/// 1=時計回り(UP), -1=反時計回り(DOWN), 0=無効遷移
const KNOBDIR: [i8; 16] = [0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0];

/// ラッチ位置: A=1, B=1 (state=3) — エンコーダーのデテント(クリック)位置
/// ../WiFiKey の FOUR3 モードに相当: このstateに達したときだけカウント
const LATCH: u8 = 3;

/// デバウンス最小間隔 (ms): ラッチ到達後これより短い連続イベントは無視
const DEBOUNCE_MS: u32 = 10;

pub struct QuadratureDecoder {
    /// 前回の (A, B) 状態 (下位2bit)
    state: u8,
    /// ラッチ到達直前の方向を蓄積 (+1 or -1)
    pending_dir: i8,
    /// 前回ステップのタイムスタンプ (ms)
    last_step_ms: u32,
    /// 前回ステップからの経過時間 (ms), 初回は0
    pub last_interval_ms: u32,
    initialized: bool,
}

impl QuadratureDecoder {
    pub fn new() -> Self {
        QuadratureDecoder {
            state: LATCH, // 起動時はラッチ位置にいると仮定
            pending_dir: 0,
            last_step_ms: 0,
            last_interval_ms: 0,
            initialized: false,
        }
    }

    /// A/B信号を更新し、デテント位置(ラッチ)に達した場合に方向を返す
    ///
    /// ../WiFiKey の RotaryEncoder (FOUR3 mode) と同等:
    /// - 中間状態の遷移は pending_dir に蓄積するだけ
    /// - state==LATCH(=3) に達したときだけ結果を確定・返却
    /// - DEBOUNCE_MS 以内の連続ラッチは無視
    pub fn tick(&mut self, a: bool, b: bool, now_ms: u32) -> Option<i8> {
        let new_ab = ((a as u8) << 1) | (b as u8);
        let idx = ((self.state & 0x03) << 2) | new_ab;
        self.state = new_ab;
        let dir = KNOBDIR[idx as usize];

        if dir != 0 {
            // 方向を蓄積 (複数遷移を加算して正味の方向を保持)
            self.pending_dir += dir;
        }

        if new_ab == LATCH && self.pending_dir != 0 {
            // ラッチ位置に到達 → 方向確定
            let resolved = if self.pending_dir > 0 { 1i8 } else { -1i8 };
            self.pending_dir = 0;

            let interval = now_ms.wrapping_sub(self.last_step_ms);
            if self.initialized && interval < DEBOUNCE_MS {
                return None; // デバウンス
            }
            if self.initialized {
                self.last_interval_ms = interval;
            }
            self.last_step_ms = now_ms;
            self.initialized = true;
            Some(resolved)
        } else {
            None
        }
    }

    /// ステップ間隔から速度倍率を計算 (MAINエンコーダー専用)
    /// - < 10ms  → ×10
    /// - < 20ms  → ×5
    /// - < 40ms  → ×2
    /// - それ以上 → ×1
    pub fn velocity_multiplier(interval_ms: u32) -> u8 {
        if interval_ms < 10 {
            10
        } else if interval_ms < 20 {
            5
        } else if interval_ms < 40 {
            2
        } else {
            1
        }
    }
}
