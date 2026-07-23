from __future__ import annotations

import sys
_CTRL_COMM_KEY = "Meta" if sys.platform == "darwin" else "Control"

class Keys:
    BACKSPACE = "Backspace"
    TAB = "Tab"
    ENTER = "Enter"
    RETURN = "Enter"
    SHIFT = "Shift"
    CONTROL = "Control"
    CTRL = "Control"
    ALT = "Alt"
    ESCAPE = "Escape"
    ESC = "Escape"
    SPACE = " "
    META = "Meta"
    COMMAND = "Meta"
    DELETE = "Delete"
    DEL = "Delete"

    CTRL_COMM = _CTRL_COMM_KEY
    CTRL_A = (CTRL_COMM, "a")
    CTRL_C = (CTRL_COMM, "c")
    CTRL_X = (CTRL_COMM, "x")
    CTRL_V = (CTRL_COMM, "v")
    CTRL_Z = (CTRL_COMM, "z")
    CTRL_Y = (CTRL_COMM, "y")
