-- ICOM IC-7300 CI-V スクリプト
-- シリアル設定: 19200bps, 1stop, no parity (IC-7300 メニューのデフォルト値)
--
-- メニュー推奨設定:
--   MENU > SET > Connectors > CI-V > CI-V Baud Rate: 19200
--   MENU > SET > Connectors > CI-V > CI-V USB Echo Back: ON
--   MENU > SET > Connectors > CI-V > CI-V Transceive: OFF (unsolicited 送信抑制)
--
-- ATU:
--   IC-7300 内蔵 ATU を使用する場合は ctl:assert_atu() 用の GPIO ピンを
--   IC-7300 の [TUNER] ボタン相当に接続してください（マニュアルATUトリガー方式）。
--
-- エンコーダー ID (board_esp32_wrover のみ):
--   0 = Fine VFO   (100Hz/step)
--   1 = Coarse VFO (1kHz/step)
--   2 = MODE       (循環切替)
--   3 = BAND       (±1MHz/step)
--
-- ボタン (button_id=0):
--   < 500ms    → モード切替 (次のモードへ)
--   500-2000ms → 周波数キャッシュ再同期 (リグから現在周波数を読み直す)
--   > 2000ms   → ATU 起動

local rig = {}

rig.serial_config = {
    baud        = 19200,
    stop_bits   = 1,
    parity      = "none",
    timeout_ms  = 200,
}

-- CI-V アドレス (IC-7300 デフォルト 0x94; メニューで変更可)
local CIV_ADDR  = 0x94
local CTRL_ADDR = 0xE0  -- コントローラアドレス (PC 側)

-- モード一覧 (循環切替用)
local MODES = { "LSB", "USB", "CW", "CW-R", "AM", "FM" }

local mode_to_civ = {
    ["LSB"]    = 0x00,
    ["USB"]    = 0x01,
    ["AM"]     = 0x02,
    ["CW"]     = 0x03,
    ["RTTY"]   = 0x04,
    ["FM"]     = 0x05,
    ["CW-R"]   = 0x07,
    ["RTTY-R"] = 0x08,
}
local civ_to_mode = {}
for k, v in pairs(mode_to_civ) do
    if not civ_to_mode[v] then civ_to_mode[v] = k end
end

-- ========== CI-V ヘルパー ==========

local function civ_frame(cmd, sub_cmd, data)
    local frame = string.char(0xFE, 0xFE, CIV_ADDR, CTRL_ADDR, cmd)
    if sub_cmd then frame = frame .. string.char(sub_cmd) end
    if data    then frame = frame .. data                  end
    return frame .. string.char(0xFD)
end

-- レスポンス受信
-- unsolicited ブロードキャスト・エコーバックをスキップし、自分宛フレームを返す
-- (FE FE E0 <CIV_ADDR> ... FD)
local function civ_read(self)
    for _ = 1, 8 do
        local buf = self.port:read_until("\xFD", 500)
        if #buf < 6 then break end
        local i = 1
        while i <= #buf - 4 do
            if buf:byte(i)   == 0xFE and buf:byte(i+1) == 0xFE
            and buf:byte(i+2) == CTRL_ADDR and buf:byte(i+3) == CIV_ADDR then
                local j = i + 4
                while j <= #buf do
                    if buf:byte(j) == 0xFD then return buf:sub(i, j) end
                    j = j + 1
                end
            end
            i = i + 1
        end
        -- このフレームは自分宛でなかった → 次のフレームを試みる
    end
    error("CI-V: no valid response received")
end

-- コマンド送信 → 応答受信
local function civ_command(self, cmd, sub_cmd, data)
    self.port:clear_input()
    self.port:write(civ_frame(cmd, sub_cmd, data))
    self.port:flush()
    return civ_read(self)
end

-- ========== BCD 変換 ==========

-- 周波数: リトルエンディアン 5バイト BCD (IC-7300 形式)
-- 例: 7.100MHz = [00, 01, 00, 10, 70] (10Hz〜GHz)
local function bcd_to_freq(s)
    local f = 0
    for i = #s, 1, -1 do
        local b = s:byte(i)
        f = f * 100 + math.floor(b / 16) * 10 + (b % 16)
    end
    return f
end

local function freq_to_bcd(freq)
    local bytes = {}
    for _ = 1, 5 do
        local lo = freq % 10;      freq = math.floor(freq / 10)
        local hi = freq % 10;      freq = math.floor(freq / 10)
        table.insert(bytes, string.char(hi * 16 + lo))
    end
    return table.concat(bytes)
end

-- パワー・SWR: ビッグエンディアン 2バイト BCD (0x0000〜0x0255)
-- 例: 255 (100%) = [0x02, 0x55], 25 (10W相当) = [0x00, 0x25]
local function bcd2_to_num(s)
    local h  = s:byte(1)
    local lu = s:byte(2)
    return h * 100 + math.floor(lu / 16) * 10 + (lu % 16)
end

local function num_to_bcd2(n)  -- n: 0〜255
    local h = math.floor(n / 100)
    local t = math.floor((n % 100) / 10)
    local u = n % 10
    return string.char(h, t * 16 + u)
end

-- ========== リグ操作 ==========

function rig:get_freq(vfoa)
    local resp = civ_command(self, 0x03, nil, nil)
    return bcd_to_freq(resp:sub(6, 10))
end

function rig:set_freq(vfoa, freq)
    civ_command(self, 0x05, nil, freq_to_bcd(freq))
end

function rig:get_mode()
    local resp = civ_command(self, 0x04, nil, nil)
    local code = resp:byte(6)
    return civ_to_mode[code] or string.format("?0x%02X", code)
end

function rig:set_mode(mode_str)
    local code = mode_to_civ[mode_str]
    if not code then error("Unknown mode: " .. mode_str) end
    civ_command(self, 0x06, code, nil)
end

-- パワー取得 (0〜100%)
function rig:get_power()
    local resp = civ_command(self, 0x14, 0x0A, nil)
    return math.floor(bcd2_to_num(resp:sub(7, 8)) * 100 / 255)
end

-- パワー設定 (0〜100%)
function rig:set_power(percent)
    local raw = math.floor(math.max(0, math.min(100, percent)) * 255 / 100)
    civ_command(self, 0x14, 0x0A, num_to_bcd2(raw))
end

-- SWR 読み取り (0〜240: 0=SWR1.0, 240=SWR∞)
function rig:read_swr()
    local resp = civ_command(self, 0x15, 0x12, nil)
    return bcd2_to_num(resp:sub(7, 8))
end

function rig:encoder_up(main, step)
    error("encoder_up: use on_encoder callback instead")
end
function rig:encoder_down(main, step)
    error("encoder_down: use on_encoder callback instead")
end

-- ========== 初期化 ==========

local cached_mode = nil
local cached_freq = nil

function rig:on_init()
    local ok, m = pcall(function() return self:get_mode() end)
    if ok then
        cached_mode = m
    else
        cached_mode = "USB"
    end
    local ok2, f = pcall(function() return self:get_freq(true) end)
    if ok2 then cached_freq = f end
    log_info("[init] mode=" .. cached_mode .. " freq=" .. tostring(cached_freq))
end

-- ========== エンコーダーイベント ==========

function rig.on_encoder(self, encoder_id, direction, steps)
    if encoder_id == 0 then
        -- Fine: 100Hz/step
        if not cached_freq then
            local ok, f = pcall(function() return self:get_freq(true) end)
            if not ok then return end
            cached_freq = f
        end
        cached_freq = cached_freq + direction * steps * 100
        pcall(function() self:set_freq(true, cached_freq) end)

    elseif encoder_id == 1 then
        -- Coarse: 1kHz/step
        if not cached_freq then
            local ok, f = pcall(function() return self:get_freq(true) end)
            if not ok then return end
            cached_freq = f
        end
        cached_freq = cached_freq + direction * steps * 1000
        pcall(function() self:set_freq(true, cached_freq) end)

    elseif encoder_id == 2 then
        -- MODE 循環切替
        local idx = 1
        for i, m in ipairs(MODES) do
            if m == cached_mode then idx = i; break end
        end
        if direction > 0 then
            idx = (idx % #MODES) + 1
        else
            idx = ((idx - 2 + #MODES) % #MODES) + 1
        end
        cached_mode = MODES[idx]
        pcall(function() self:set_mode(cached_mode) end)

    elseif encoder_id == 3 then
        -- BAND: ±1MHz/step
        if not cached_freq then
            local ok, f = pcall(function() return self:get_freq(true) end)
            if not ok then return end
            cached_freq = f
        end
        cached_freq = cached_freq + direction * steps * 1000000
        pcall(function() self:set_freq(true, cached_freq) end)
    end
end

-- ========== ボタンイベント ==========

function rig.on_button(self, button_id, press_ms)
    if button_id ~= 0 then return end

    if press_ms < 500 then
        -- 短押し: モード切替 (次のモードへ)
        rig.on_encoder(self, 2, 1, 1)

    elseif press_ms < 2000 then
        -- 中押し: 周波数キャッシュ再同期
        local ok, f = pcall(function() return self:get_freq(true) end)
        if ok then
            cached_freq = f
            log_info("[sync] freq=" .. f)
        end

    else
        -- 長押し: ATU 起動
        if rig.actions and rig.actions.start_atu then
            rig.actions.start_atu.fn(self, rig_control)
        end
    end
end

-- ========== アクション ==========

rig.actions = {
    start_atu = {
        label = "ATU",
        fn = function(self, ctl)
            log_info("[ATU] === ATU tuning start ===")

            log_info("[ATU] 1/6 Saving current settings...")
            local saved_power = self:get_power()
            local saved_mode  = self:get_mode()
            log_info("[ATU] saved: mode=" .. saved_mode .. " power=" .. saved_power .. "%")

            log_info("[ATU] 2/6 Setting CW 10W...")
            self:set_mode("CW")
            self:set_power(10)  -- IC-7300: 10% = 10W (最大100W)
            sleep_ms(500)

            log_info("[ATU] 3/6 Key ON, sending ATU pulse...")
            ctl:assert_key(true)
            local ok, err = pcall(function()
                sleep_ms(100)
                ctl:assert_atu(true)
                sleep_ms(500)
                ctl:assert_atu(false)

                log_info("[ATU] 4/6 Monitoring SWR...")
                local swr_count = 0
                for i = 1, 20 do
                    local rok, swr = pcall(function() return self:read_swr() end)
                    if not rok then
                        log_info("[ATU] SWR read error: " .. tostring(swr))
                        break
                    end
                    -- ICOM SWR scale: 0=SWR1.0, 240=SWR∞ (Yaesu の 0-255 と異なる)
                    log_info(string.format("[ATU] SWR [%d] = %d (good: %d/2)", i, swr, swr_count))
                    if swr < 80 then
                        swr_count = swr_count + 1
                    else
                        swr_count = 0
                    end
                    if swr_count >= 2 then
                        log_info("[ATU] SWR converged!")
                        break
                    end
                    sleep_ms(100)
                end
            end)

            -- ATU がチューニング結果をラッチするまで待つ
            sleep_ms(2000)

            log_info("[ATU] 5/6 Key OFF")
            ctl:assert_key(false)

            log_info("[ATU] 6/6 Restoring mode=" .. saved_mode .. " power=" .. saved_power .. "%...")
            sleep_ms(500)
            pcall(function() self:set_mode(saved_mode) end)
            pcall(function() self:set_power(saved_power) end)

            if ok then
                log_info("[ATU] === ATU tuning complete ===")
            else
                log_info("[ATU] === ATU tuning FAILED: " .. tostring(err) .. " ===")
                error(err)
            end
        end,
    },

    -- Fine VFO 操作 (100Hz/step)
    fine_up = {
        label = "Fine ▲",
        fn = function(self, _ctl) rig.on_encoder(self, 0,  1, 1) end,
    },
    fine_down = {
        label = "Fine ▼",
        fn = function(self, _ctl) rig.on_encoder(self, 0, -1, 1) end,
    },

    -- Coarse VFO 操作 (1kHz/step)
    coarse_up = {
        label = "Coarse ▲",
        fn = function(self, _ctl) rig.on_encoder(self, 1,  1, 1) end,
    },
    coarse_down = {
        label = "Coarse ▼",
        fn = function(self, _ctl) rig.on_encoder(self, 1, -1, 1) end,
    },

    -- モード切替
    mode_next = {
        label = "Mode ▶",
        fn = function(self, _ctl) rig.on_encoder(self, 2,  1, 1) end,
    },
    mode_prev = {
        label = "◀ Mode",
        fn = function(self, _ctl) rig.on_encoder(self, 2, -1, 1) end,
    },

    -- バンド切替 (±1MHz)
    band_up = {
        label = "Band ▲",
        fn = function(self, _ctl) rig.on_encoder(self, 3,  1, 1) end,
    },
    band_down = {
        label = "Band ▼",
        fn = function(self, _ctl) rig.on_encoder(self, 3, -1, 1) end,
    },

    -- 周波数キャッシュ再同期
    sync_freq = {
        label = "Sync",
        fn = function(self, _ctl)
            local ok, f = pcall(function() return self:get_freq(true) end)
            if ok then
                cached_freq = f
                log_info("[sync] freq=" .. f)
            end
        end,
    },
}

return rig
