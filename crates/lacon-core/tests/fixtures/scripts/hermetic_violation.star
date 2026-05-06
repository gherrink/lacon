# Attempts to use load() — must be rejected by the hermetic runtime.
load("nope.bzl", "nope")
def process(ctx, lines):
    return lines
