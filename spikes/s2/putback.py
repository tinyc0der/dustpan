"""Check Finder Put Back for spike items in the Trash, through agent-desktop.

Usage: python3 putback.py LIST_FILE

LIST_FILE holds one "NAME<TAB>ORIGINAL_PATH" per line, for items this spike put
in the Trash. It presses File > Put Back when Finder enables it, so items move
out of the Trash, and checks whether each one is back at ORIGINAL_PATH. It
clicks OK on any Finder alert that Put Back raises and records the alert text.
Prints NAME<TAB>MENU_STATE<TAB>RESULT<TAB>ALERT per line.
"""

import json
import os
import subprocess
import sys
import time

DEBUG = bool(os.environ.get("PUTBACK_DEBUG"))


def ad(*args):
    r = subprocess.run(["agent-desktop", *args], capture_output=True, text=True)
    if DEBUG:
        print("DBG", args, r.stdout[:400], file=sys.stderr)
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError:
        return {"ok": False, "error": {"code": "BAD_OUTPUT", "message": r.stdout + r.stderr}}


def nodes(tree):
    stack = [(tree, [])]
    while stack:
        n, anc = stack.pop()
        yield n, anc
        stack.extend((c, anc + [n]) for c in n.get("children", []))


def osa(script):
    return subprocess.run(["osascript", "-e", script], capture_output=True, text=True).stdout.strip()


def finder_windows():
    return {w["id"]: w for w in (ad("list-windows", "--app", "Finder").get("data") or [])}


def row_ref(window, name):
    snap = ad("snapshot", "--window-id", window)
    if not snap.get("ok"):
        return None
    for n, anc in nodes(snap["data"]["tree"]):
        if (n.get("value") or n.get("name")) == name and n.get("role") == "textfield":
            rows = [a for a in anc if a.get("role") in ("treeitem", "row")]
            return rows[-1]["ref_id"] if rows else n["ref_id"]
    return None


def menu_item(name):
    menu = ad("snapshot", "--app", "Finder", "--surface", "menu")
    if menu.get("ok"):
        for n, _ in nodes(menu["data"]["tree"]):
            if n.get("name") == name:
                return n
    return None


def file_menu_ref():
    bar = ad("snapshot", "--app", "Finder", "--surface", "menubar", "-i")
    if not bar.get("ok"):
        return None
    return next((n["ref_id"] for n, _ in nodes(bar["data"]["tree"]) if n.get("name") == "File" and n.get("ref_id")), None)


def dismiss_new_alerts(before):
    texts = []
    for wid, w in finder_windows().items():
        if wid in before or w["bounds"]["height"] > 300:
            continue
        snap = ad("snapshot", "--window-id", wid)
        if not snap.get("ok"):
            continue
        texts += [n.get("value") or n.get("name") for n, _ in nodes(snap["data"]["tree"]) if n.get("role") == "statictext"]
        ok = next((n["ref_id"] for n, _ in nodes(snap["data"]["tree"]) if n.get("role") == "button" and n.get("name") == "OK"), None)
        if ok:
            ad("click", ok)
    return " | ".join(t for t in texts if t)


def check(window, name, original):
    ref = row_ref(window, name)
    if not ref:
        return "row-not-found", "-", ""
    ad("click", ref)
    if not ad("is", ref, "--property", "selected").get("data", {}).get("result"):
        return "not-selected", "-", ""
    ad("focus-window", "--window-id", window)
    file_ref = file_menu_ref()
    if not file_ref:
        return "no-file-menu", "-", ""
    ad("click", file_ref)
    item = menu_item("Put Back")
    if item is None:
        ad("click", file_ref)
        return "no-put-back-item", "-", ""
    if "disabled" in (item.get("states") or []):
        ad("click", file_ref)
        return "disabled", "not-restored", ""
    before = set(finder_windows())
    ad("click", item["ref_id"])
    for _ in range(20):
        if os.path.lexists(original):
            break
        time.sleep(0.25)
    time.sleep(0.5)
    alert = dismiss_new_alerts(before)
    return "enabled", "restored" if os.path.lexists(original) else "not-restored", alert


def main():
    pairs = [line.split("\t") for line in open(sys.argv[1]).read().splitlines() if line]
    finder_id = osa('tell application "Finder" to return id of (make new Finder window to trash)')
    window = f"w-{finder_id}"
    for _ in range(20):
        if window in finder_windows():
            break
        time.sleep(0.25)
    ad("focus-window", "--window-id", window)
    time.sleep(1.5)
    for name, original in pairs:
        state, result, alert = check(window, name, original)
        print(f"{name}\t{state}\t{result}\t{alert}", flush=True)
    osa(f'tell application "Finder" to close Finder window id {finder_id}')


main()
