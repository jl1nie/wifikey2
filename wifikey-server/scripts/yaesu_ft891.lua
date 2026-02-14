-- Yaesu FT-891 CAT Protocol Script
-- self.port is set by Rust before any function is called

local rig = {}

-- シリアルポート設定
rig.serial_config = {
    baud = 4800,
    stop_bits = 2,
    parity = "none",
    timeout_ms = 100,
}

-- モード文字列 → CATコード変換テーブル
local mode_to_cat = {
    ["LSB"]       = "1",
    ["USB"]       = "2",
    ["CW-U"]      = "3",
    ["FM"]        = "4",
    ["AM"]        = "5",
    ["RTTY-L"]    = "6",
    ["CW-L"]      = "7",
    ["DATA-L"]    = "8",
    ["RTTY-U"]    = "9",
    ["DATA-FM"]   = "A",
    ["FM-N"]      = "B",
    ["DATA-U"]    = "C",
    ["AM-N"]      = "D",
    ["PSK"]       = "E",
    ["DATA-FM-N"] = "F",
}

-- CATコード → モード文字列変換テーブル
local cat_to_mode = {}
for k, v in pairs(mode_to_cat) do
    cat_to_mode[v] = k
end

-- CAT コマンド書き込み
local function cat_write(self, command)
    log_info("cat write " .. command)
    self.port:write(command)
end

-- CAT コマンド読み取り（コマンド送信→応答受信）
local function cat_read(self, command)
    self.port:clear_input()
    self.port:write(command)
    local buf = self.port:read(1024)
    local prefix = command:sub(1, 2)
    local idx = buf:find(prefix, 1, true)
    if not idx then
        error("cat read error buffer=" .. buf)
    end
    local res = buf:sub(idx)
    log_info(string.format("cat cmd %s read %s(%d)", command, res, #res))
    return res
end

-- 周波数取得
function rig:get_freq(vfoa)
    local cmd = vfoa and "FA;" or "FB;"
    local fstr = cat_read(self, cmd)
    local freq = tonumber(fstr:sub(3, 11))
    if not freq then
        error("CAT read freq failed. " .. fstr:sub(3, 11))
    end
    return freq
end

-- 周波数設定
function rig:set_freq(vfoa, freq)
    if freq < 30000 or freq > 75000000 then
        error("Parameter out of range: freq=" .. freq)
    end
    local vfo = vfoa and "A" or "B"
    cat_write(self, string.format("F%s%09d;", vfo, freq))
end

-- パワー取得
function rig:get_power()
    local pstr = cat_read(self, "PC;")
    local pwr = tonumber(pstr:sub(3, 5))
    if not pwr then
        error("CAT read power failed. " .. pstr:sub(3, 5))
    end
    return pwr
end

-- パワー設定
function rig:set_power(power)
    if power < 5 or power > 100 then
        error("Parameter out of range: power=" .. power)
    end
    cat_write(self, string.format("PC%03d;", power))
end

-- エンコーダ上
function rig:encoder_up(main, step)
    if step < 1 or step > 99 then
        error("Parameter out of range: step=" .. step)
    end
    local vfo = main and 0 or 1
    cat_write(self, string.format("EU%d%02d;", vfo, step))
end

-- エンコーダ下
function rig:encoder_down(main, step)
    if step < 1 or step > 99 then
        error("Parameter out of range: step=" .. step)
    end
    local vfo = main and 0 or 1
    cat_write(self, string.format("ED%d%02d;", vfo, step))
end

-- モード設定
function rig:set_mode(mode_str)
    local code = mode_to_cat[mode_str]
    if not code then
        error("Unknown mode: " .. mode_str)
    end
    cat_write(self, string.format("MD0%s;", code))
end

-- モード取得
function rig:get_mode()
    local mstr = cat_read(self, "MD0;")
    local code = mstr:sub(4, 4)
    local mode = cat_to_mode[code]
    if not mode then
        error("Unknown mode code: " .. code)
    end
    return mode
end

-- SWR読み取り
function rig:read_swr()
    local mstr = cat_read(self, "RM6;")
    local swr = tonumber(mstr:sub(4, 6))
    if not swr then
        error("CAT read fail. swr=" .. mstr)
    end
    return swr
end

return rig
