-- FTDX10 CAT スクリプト
-- シリアル設定: 38400bps, 1stop, no parity
-- (FTDX10 メニュー: MENU > CAT > CAT RATE を 38400bps に設定すること)
--
-- エンコーダーID:
--   0 = Fine   (100Hz/step)
--   1 = Coarse (1kHz/step = 100Hz * 10)
--   2 = MODE   (モード切替)
--   3 = BAND   (バンド切替)
--
-- ボタン (button_id=0):
--   < 500ms      → RIT トグル (RT;)
--   500-2000ms   → VFO A/B スワップ (SV;)
--   > 2000ms     → ATU 起動 (start_atu)

local rig = {}

-- シリアルポート設定
rig.serial_config = {
    baud        = 38400,
    stop_bits   = 1,
    parity      = "none",
    timeout_ms  = 200,
}

-- モード一覧（循環切替）
local MODES = {"LSB", "USB", "CW", "FM", "AM", "RTTY-L", "CW-R"}

-- 現在のモードキャッシュ（get_mode()のシリアル読み取りを省くため）
local cached_mode = nil



-- モードコード (MD0x;) → モード文字列
local mode_to_cat = {
    ["LSB"]    = "1",
    ["USB"]    = "2",
    ["CW"]     = "3",
    ["FM"]     = "4",
    ["AM"]     = "5",
    ["RTTY-L"] = "6",
    ["CW-R"]   = "7",
}
local cat_to_mode = {}
for k, v in pairs(mode_to_cat) do
    cat_to_mode[v] = k
end

-- ========== ヘルパー ==========

local function hex_str(s)
    local hex = {}
    for i = 1, #s do
        hex[#hex + 1] = string.format("%02X", s:byte(i))
    end
    return table.concat(hex, " ")
end

--- CAT コマンド送信 (応答なし)
local function cat_write(self, command)
    log_trace("[CAT TX] '" .. command .. "'")
    self.port:write(command)
end

--- CAT コマンド送信 → 応答受信 (FT891と同じパターン)
--- バッファクリア → コマンド送信 → ";" まで読み取り → プレフィックス確認
local function cat_read(self, command)
    log_trace("[CAT RX] cmd='" .. command .. "'")
    self.port:clear_input()
    self.port:write(command)
    self.port:flush()  -- TX スレッドが送信完了するまで待ってから読む
    local buf = self.port:read_until(";", 500)
    log_trace("[CAT RX] resp='" .. buf .. "' hex=" .. hex_str(buf))
    local prefix = command:sub(1, 2)
    local idx = buf:find(prefix, 1, true)
    if not idx then
        log_info("[CAT RX] prefix '" .. prefix .. "' not found in: '" .. buf .. "'")
        error("cat read error: prefix '" .. prefix .. "' not found in buffer '" .. buf .. "'")
    end
    local res = buf:sub(idx)
    log_trace("[CAT RX] parsed='" .. res .. "'")
    return res
end

-- ========== CAT 操作 ==========

function rig:get_freq(vfoa)
    local cmd = vfoa and "FA;" or "FB;"
    local resp = cat_read(self, cmd)
    local freq = tonumber(resp:sub(3, 11))
    if not freq then error("get_freq failed: '" .. resp .. "'") end
    return freq
end

function rig:set_freq(vfoa, freq)
    local vfo = vfoa and "A" or "B"
    cat_write(self, string.format("F%s%09d;", vfo, freq))
end

function rig:get_power()
    local resp = cat_read(self, "PC;")
    local pwr = tonumber(resp:sub(3, 5))
    if not pwr then error("get_power failed: '" .. resp .. "'") end
    return pwr
end

function rig:set_power(power)
    cat_write(self, string.format("PC%03d;", power))
end

function rig:get_mode()
    local resp = cat_read(self, "MD0;")
    local code = resp:sub(4, 4)
    local mode = cat_to_mode[code]
    if not mode then error("unknown mode code: '" .. code .. "' in '" .. resp .. "'") end
    return mode
end

function rig:set_mode(mode_str)
    local code = mode_to_cat[mode_str]
    if not code then
        log_info("[set_mode] unknown mode: " .. tostring(mode_str))
        return
    end
    cat_write(self, "MD0" .. code .. ";")
end

function rig:read_swr()
    local resp = cat_read(self, "RM6;")
    local swr = tonumber(resp:sub(4, 6))
    if not swr then error("read_swr failed: '" .. resp .. "'") end
    return swr
end

-- ========== 初期化 ==========

--- 起動時に1回呼ばれる（モードキャッシュを事前取得）
function rig:on_init()
    local ok, m = pcall(function() return self:get_mode() end)
    if ok then
        cached_mode = m
        log_info("[init] mode cached: " .. m)
    else
        cached_mode = "USB"
        log_info("[init] get_mode failed, defaulting to USB: " .. tostring(m))
    end
end

-- ========== エンコーダーイベント ==========

function rig.on_encoder(self, encoder_id, direction, steps)
    if encoder_id == 0 then
        -- Fine: 100Hz/step (velocity multiplier適用済み)
        if direction > 0 then
            cat_write(self, string.format("EU0%02d;", steps))
        else
            cat_write(self, string.format("ED0%02d;", steps))
        end

    elseif encoder_id == 1 then
        -- Coarse: 1kHz/step (steps*10 で渡す)
        -- EU1/ED1 は VFO-B エンコーダーで方向が逆なので direction を反転
        local coarse_steps = math.min(steps * 10, 99)
        if direction > 0 then
            cat_write(self, string.format("ED1%02d;", coarse_steps))
        else
            cat_write(self, string.format("EU1%02d;", coarse_steps))
        end

    elseif encoder_id == 2 then
        -- MODE: 循環切替（cached_modeはon_initで初期化済み）
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
        self:set_mode(cached_mode)

    elseif encoder_id == 3 then
        -- BAND: BU0; (up) / BD0; (down) — メインVFOのバンドを上下
        if direction > 0 then
            cat_write(self, "BU0;")
        else
            cat_write(self, "BD0;")
        end
    end
end

-- ========== ボタンイベント ==========

function rig.on_button(self, button_id, press_ms)
    if button_id ~= 0 then return end

    if press_ms < 500 then
        -- 短押し: RIT トグル
        cat_write(self, "RT;")
    elseif press_ms < 2000 then
        -- 中押し: VFO A/B スワップ
        cat_write(self, "SV;")
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
            log_info("[ATU] saved: mode=" .. saved_mode .. " power=" .. saved_power .. "W")

            log_info("[ATU] 2/6 Setting CW 10W...")
            self:set_mode("CW")
            self:set_power(10)
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
                    log_info(string.format("[ATU] SWR [%d] = %d (good: %d/2)", i, swr, swr_count))
                    if swr < 50 then
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

            -- ATU がチューニング結果をラッチするまで待つ（SWR 収束後も内部処理が続く）
            sleep_ms(2000)

            log_info("[ATU] 5/6 Key OFF")
            ctl:assert_key(false)

            log_info("[ATU] 6/6 Restoring mode=" .. saved_mode .. " power=" .. saved_power .. "W...")
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

    -- Fine 操作 (100Hz/step)
    fine_up = {
        label = "Fine ▲",
        fn = function(self, _ctl) self:on_encoder(0,  1, 1) end,
    },
    fine_down = {
        label = "Fine ▼",
        fn = function(self, _ctl) self:on_encoder(0, -1, 1) end,
    },
    -- Coarse 操作 (1kHz/step)
    coarse_up = {
        label = "Coarse ▲",
        fn = function(self, _ctl) self:on_encoder(1,  1, 1) end,
    },
    coarse_down = {
        label = "Coarse ▼",
        fn = function(self, _ctl) self:on_encoder(1, -1, 1) end,
    },
    -- モード切替
    mode_next = {
        label = "Mode ▶",
        fn = function(self, _ctl) self:on_encoder(2,  1, 1) end,
    },
    mode_prev = {
        label = "◀ Mode",
        fn = function(self, _ctl) self:on_encoder(2, -1, 1) end,
    },
    -- バンド切替 (BU0; / BD0;)
    band_up = {
        label = "Band ▲",
        fn = function(self, _ctl) cat_write(self, "BU0;") end,
    },
    band_down = {
        label = "Band ▼",
        fn = function(self, _ctl) cat_write(self, "BD0;") end,
    },
}

return rig
