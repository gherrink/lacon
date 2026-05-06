# Keeps only lines containing "error" or "FAIL"; appends a count summary on non-zero exit.
def process(ctx, lines):
    matches = [s for s in lines if "error" in s or "FAIL" in s]
    if ctx["exit_code"] != 0:
        matches.append("(%d errors total, exit %d)" % (len(matches), ctx["exit_code"]))
    return matches
