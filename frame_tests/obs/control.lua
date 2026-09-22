obs = obslua
local directory = os.getenv("GE_OBS_TEST_DIR")
local last_id = 0

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
    local file = assert(io.open(directory .. "/response.tmp", "w"))
    file:write(obs.obs_data_get_json(data))
    file:close()
    obs.obs_data_release(data)
    assert(os.rename(directory .. "/response.tmp", directory .. "/response.json"))
end

local function execute(command)
    local action = obs.obs_data_get_string(command, "action")
    if action == "quit" then
        local ffi = require("ffi")
        ffi.cdef("int raise(int signal);")
        ffi.C.raise(2)
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
    local file = io.open(directory .. "/command.json", "r")
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
