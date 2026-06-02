const std = @import("std");

fn log(comptime format: []const u8, args: anytype) void {
	std.debug.print("[The Golden Eye] " ++ format, args);
}

pub export fn zig_obs_module_load() bool {
	log("Hello from Zig!\n", .{});
	return true;
}

pub export fn zig_obs_module_unload() void {
	log("Goodbye from Zig!\n", .{});
}
