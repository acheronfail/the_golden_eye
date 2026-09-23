obs = obslua
local directory = os.getenv("GE_OBS_TEST_DIR")
local last_id = 0
local ffi = require("ffi")
local windows = ffi.os == "Windows"
local kernel32, user32, frontend
if windows then
    ffi.cdef([[
        int MultiByteToWideChar(unsigned int, unsigned long, const char *, int, wchar_t *, int);
        int MoveFileExW(const wchar_t *, const wchar_t *, unsigned long);
        int PostMessageW(void *, unsigned int, uintptr_t, intptr_t);
        void *obs_frontend_get_main_window_handle(void);
    ]])
    kernel32 = ffi.load("kernel32")
    user32 = ffi.load("user32")
    frontend = ffi.load("obs-frontend-api")
end

local function wide(value)
    local size = kernel32.MultiByteToWideChar(65001, 0, value, -1, nil, 0)
    assert(size > 0, "Cannot convert path to UTF-16")
    local result = ffi.new("wchar_t[?]", size)
    assert(kernel32.MultiByteToWideChar(65001, 0, value, -1, result, size) > 0)
    return result
end

local function respond(id, error_message, name)
    local data = obs.obs_data_create()
    obs.obs_data_set_int(data, "id", id)
    obs.obs_data_set_string(data, "error", error_message or "")
    if name and name ~= "" then
        local source = obs.obs_get_source_by_name(name)
        obs.obs_data_set_bool(data, "exists", source ~= nil)
        if source then
            obs.obs_data_set_int(data, "width", obs.obs_source_get_width(source))
            obs.obs_data_set_int(data, "height", obs.obs_source_get_height(source))
            obs.obs_data_set_bool(data, "paused", obs.obs_source_media_get_state(source) == obs.OBS_MEDIA_STATE_PAUSED)
            obs.obs_data_set_bool(data, "ended", obs.obs_source_media_get_state(source) == obs.OBS_MEDIA_STATE_ENDED)
            obs.obs_source_release(source)
        end
    end
    local response_path = directory .. "/response-" .. id
    local file = assert(io.open(response_path .. ".tmp", "w"))
    file:write(obs.obs_data_get_json(data))
    file:close()
    obs.obs_data_release(data)
    if windows then
        assert(kernel32.MoveFileExW(wide(response_path .. ".tmp"), wide(response_path .. ".json"), 1) ~= 0)
    else
        assert(os.rename(response_path .. ".tmp", response_path .. ".json"))
    end
end

local function execute(command)
    local action = obs.obs_data_get_string(command, "action")
    if action == "quit" then
        if windows then
            assert(user32.PostMessageW(frontend.obs_frontend_get_main_window_handle(), 0x0010, 0, 0) ~= 0)
        else
            ffi.cdef("int raise(int signal);")
            ffi.C.raise(2)
        end
        return
    end
    local name = obs.obs_data_get_string(command, "name")
    if name == "" then name = "Fixture" end
    local source = obs.obs_get_source_by_name(name)
    if action == "remove" then
        if source then
            obs.obs_source_remove(source)
            obs.obs_source_release(source)
        end
        return
    end
    if action == "status" and not source then return end
    if action == "restart" or action == "pause" or action == "status" then
        assert(source, "Fixture source is missing")
        if action == "restart" then obs.obs_source_media_restart(source) end
        if action == "pause" then obs.obs_source_media_play_pause(source, true) end
        obs.obs_source_release(source)
        return
    end
    local settings = obs.obs_data_create()
    local path = obs.obs_data_get_string(command, "path")
    local kind = obs.obs_data_get_string(command, "kind")
    if kind == "image_source" then
        obs.obs_data_set_string(settings, "file", path)
    else
        obs.obs_data_set_bool(settings, "is_local_file", true)
        obs.obs_data_set_string(settings, "local_file", path)
        obs.obs_data_set_bool(settings, "looping", false)
        obs.obs_data_set_bool(settings, "restart_on_activate", false)
        obs.obs_data_set_bool(settings, "clear_on_media_end", false)
        obs.obs_data_set_bool(settings, "hw_decode", false)
        obs.obs_data_set_int(settings, "speed_percent", obs.obs_data_get_int(command, "speedPercent"))
    end
    if source and obs.obs_source_get_id(source) ~= kind then
        obs.obs_source_release(source)
        obs.obs_data_release(settings)
        error("Remove the old source before changing its kind")
    end
    if source then
        obs.obs_source_update(source, settings)
    else
        source = assert(obs.obs_source_create(kind, name, settings, nil), "Cannot create fixture source")
        local scene_source = assert(obs.obs_frontend_get_current_scene(), "No current scene")
        obs.obs_scene_add(obs.obs_scene_from_source(scene_source), source)
        obs.obs_source_release(scene_source)
    end
    obs.obs_source_release(source)
    obs.obs_data_release(settings)
end

local function poll()
    local file = io.open(directory .. "/command-" .. (last_id + 1) .. ".json", "r")
    if not file then return end
    local command = obs.obs_data_create_from_json(file:read("*a"))
    file:close()
    if not command then return end
    local id = obs.obs_data_get_int(command, "id")
    if id > last_id then
        last_id = id
        local ok, err = pcall(execute, command)
        respond(id, ok and "" or tostring(err), obs.obs_data_get_string(command, "name"))
    end
    obs.obs_data_release(command)
end

function script_load(settings)
    assert(directory, "GE_OBS_TEST_DIR is required")
    respond(0)
    obs.timer_add(poll, 50)
end

function script_unload()
    obs.timer_remove(poll)
end
