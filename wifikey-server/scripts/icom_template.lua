-- ICOM CI-V Protocol Template
-- CI-Vアドレスやコマンドは機種ごとに異なるため、必要に応じて変更してください。
-- self.port is set by Rust before any function is called.

local rig = {}

-- シリアルポート設定 (ICOM 典型値)
rig.serial_config = {
    baud = 9600,
    stop_bits = 1,
    parity = "none",
    timeout_ms = 100,
}

-- CI-V設定 (機種に合わせて変更)
local CIV_ADDR = 0x94     -- リグのCI-Vアドレス (例: IC-7300 = 0x94)
local CTRL_ADDR = 0xE0    -- コントローラアドレス

-- CI-Vフレーム構築
local function civ_frame(cmd, sub_cmd, data)
    local frame = string.char(0xFE, 0xFE, CIV_ADDR, CTRL_ADDR, cmd)
    if sub_cmd then
        frame = frame .. string.char(sub_cmd)
    end
    if data then
        frame = frame .. data
    end
    frame = frame .. string.char(0xFD)
    return frame
end

-- CI-V応答読み取り
local function civ_read(self)
    local buf = self.port:read_until("\xFD", 2000)
    -- エコーバックをスキップし、リグからの応答を探す
    -- 応答は FE FE E0 <rig_addr> ... FD
    local i = 1
    while i <= #buf - 4 do
        if buf:byte(i) == 0xFE and buf:byte(i+1) == 0xFE
           and buf:byte(i+2) == CTRL_ADDR and buf:byte(i+3) == CIV_ADDR then
            -- 応答の終端を探す
            local j = i + 4
            while j <= #buf do
                if buf:byte(j) == 0xFD then
                    return buf:sub(i, j)
                end
                j = j + 1
            end
        end
        i = i + 1
    end
    error("CI-V: no valid response received")
end

-- CI-Vコマンド送信→応答受信
local function civ_command(self, cmd, sub_cmd, data)
    self.port:clear_input()
    local frame = civ_frame(cmd, sub_cmd, data)
    self.port:write(frame)
    return civ_read(self)
end

-- BCD周波数 → 数値変換 (5バイトBCD, リトルエンディアン)
local function bcd_to_freq(data)
    local freq = 0
    for i = #data, 1, -1 do
        local b = data:byte(i)
        freq = freq * 100 + math.floor(b / 16) * 10 + (b % 16)
    end
    return freq
end

-- 数値 → BCD周波数変換 (5バイト)
local function freq_to_bcd(freq)
    local bytes = {}
    for _ = 1, 5 do
        local low = freq % 10
        freq = math.floor(freq / 10)
        local high = freq % 10
        freq = math.floor(freq / 10)
        table.insert(bytes, string.char(high * 16 + low))
    end
    return table.concat(bytes)
end

-- モードマッピング (CI-V)
local mode_to_civ = {
    ["LSB"]    = 0x00,
    ["USB"]    = 0x01,
    ["AM"]     = 0x02,
    ["CW-U"]   = 0x03,
    ["RTTY-U"] = 0x04,
    ["FM"]     = 0x05,
    ["CW-L"]   = 0x07,
    ["RTTY-L"] = 0x08,
    ["DATA-U"] = 0x01,  -- USBベースのデータモード
    ["DATA-L"] = 0x00,  -- LSBベースのデータモード
}

local civ_to_mode = {}
for k, v in pairs(mode_to_civ) do
    if not civ_to_mode[v] then
        civ_to_mode[v] = k
    end
end

-- 周波数取得
function rig:get_freq(vfoa)
    -- TODO: VFO A/B選択の実装
    local resp = civ_command(self, 0x03, nil, nil)
    local data = resp:sub(6, 10)  -- 5バイトBCDデータ
    return bcd_to_freq(data)
end

-- 周波数設定
function rig:set_freq(vfoa, freq)
    -- TODO: VFO A/B選択の実装
    local data = freq_to_bcd(freq)
    civ_command(self, 0x05, nil, data)
end

-- パワー取得
function rig:get_power()
    local resp = civ_command(self, 0x14, 0x0A, nil)
    -- パワー値は2バイトBCD (0000-0255)
    local data = resp:sub(7, 8)
    local raw = bcd_to_freq(data)  -- BCD → 数値変換を流用
    -- ICOMのパワー値はスケーリングが必要（機種依存）
    return math.floor(raw * 100 / 255)
end

-- パワー設定
function rig:set_power(power)
    local raw = math.floor(power * 255 / 100)
    local high = math.floor(raw / 100)
    local mid = math.floor((raw % 100) / 10)
    local low = raw % 10
    local data = string.char(mid * 16 + low, high)
    civ_command(self, 0x14, 0x0A, data)
end

-- エンコーダ上（ICOMでは未対応の場合あり）
function rig:encoder_up(main, step)
    error("encoder_up not implemented for ICOM CI-V")
end

-- エンコーダ下（ICOMでは未対応の場合あり）
function rig:encoder_down(main, step)
    error("encoder_down not implemented for ICOM CI-V")
end

-- モード設定
function rig:set_mode(mode_str)
    local code = mode_to_civ[mode_str]
    if not code then
        error("Unknown mode: " .. mode_str)
    end
    civ_command(self, 0x06, code, nil)
end

-- モード取得
function rig:get_mode()
    local resp = civ_command(self, 0x04, nil, nil)
    local code = resp:byte(6)
    local mode = civ_to_mode[code]
    if not mode then
        error("Unknown CI-V mode code: " .. code)
    end
    return mode
end

-- SWR読み取り
function rig:read_swr()
    local resp = civ_command(self, 0x15, 0x12, nil)
    local data = resp:sub(7, 8)
    return bcd_to_freq(data)
end

-- ==============================
-- アクション定義
-- ==============================

local current_freq = nil  -- VM生存中ずっと保持

rig.actions = {
    start_atu = {
        label = "Start ATU",
        fn = function(self, ctl)
            log_info("[ATU/Lua] start_atu begin (ICOM)")
            local saved_power = self:get_power()
            local saved_mode = self:get_mode()

            self:set_mode("CW-U")
            self:set_power(10)
            sleep_ms(500)

            ctl:assert_key(true)
            sleep_ms(100)

            ctl:assert_atu(true)
            sleep_ms(500)
            ctl:assert_atu(false)

            local swr_count = 0
            for i = 1, 20 do
                sleep_ms(100)
                local ok, swr = pcall(function() return self:read_swr() end)
                if not ok then break end
                log_info(string.format("[ATU] SWR [%d] = %d (good: %d/3)", i, swr, swr_count))
                if swr < 50 then
                    swr_count = swr_count + 1
                else
                    swr_count = 0
                end
                if swr_count >= 3 then break end
            end

            ctl:assert_key(false)
            sleep_ms(500)
            self:set_mode(saved_mode)
            self:set_power(saved_power)
            log_info("[ATU/Lua] start_atu completed")
        end,
    },
    freq_up = {
        label = "+",
        fn = function(self, ctl)
            if not current_freq then current_freq = self:get_freq(true) end
            current_freq = current_freq + 100
            self:set_freq(true, current_freq)
        end,
    },
    freq_down = {
        label = "-",
        fn = function(self, ctl)
            if not current_freq then current_freq = self:get_freq(true) end
            current_freq = current_freq - 100
            self:set_freq(true, current_freq)
        end,
    },
}

return rig
