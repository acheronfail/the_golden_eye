const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const config_header = b.addConfigHeader(
        .{
            .style = .{ .cmake = b.path("vendor/obs/libobs/obsconfig.h.in") },
            .include_path = "obsconfig.h",
        },
        .{
            .OBS_DATA_PATH = "",
            .OBS_PLUGIN_PATH = "",
            .OBS_PLUGIN_DESTINATION = "",
            .OBS_RELEASE_CANDIDATE = 0,
            .OBS_BETA = 0,
        },
    );

    const root_module = b.createModule(.{
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = false,
    });

    root_module.addCSourceFile(.{
        .file = b.path("src/main.c"),
    });

    root_module.addIncludePath(config_header.getOutputDir());
    root_module.addIncludePath(b.path("vendor"));
    root_module.addIncludePath(b.path("vendor/obs/libobs"));

    root_module.linkSystemLibrary("obs", .{});
    root_module.linkSystemLibrary("obs-frontend-api", .{});

    const plugin = b.addLibrary(.{
        .linkage = .dynamic,
        .name = "ge",
        .root_module = root_module,
    });

    b.installArtifact(plugin);
}
