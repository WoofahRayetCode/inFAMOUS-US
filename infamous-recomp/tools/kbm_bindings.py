"""Keyboard/mouse bindings for the prompt-glyph tools (make_kbm_icons.py, make_kbm_prompts.py): the same controls.ini
that cellPad.c reads (next to settings.ini in infamous-recomp/, "action=KEY" lines), so the drawn prompts follow a
player's rebinding. Without the file the defaults below are the port's v150 map.
KEY: a letter or digit, SPACE, ENTER, TAB, ESC, SHIFT, CTRL, ALT, BACKSPACE, UP, DOWN, LEFT, RIGHT, F1-F12,
LMB, RMB, MMB, MOUSE4, MOUSE5, or VK:<hex> (any Windows virtual-key code)."""
import os
DEFAULTS = {
    'forward': 'W', 'back': 'S', 'left': 'A', 'right': 'D',
    'jump': 'SPACE', 'melee': 'F', 'triangle': 'E', 'circle': 'C',
    'aim': 'RMB', 'fire': 'LMB', 'l2': 'Q', 'r2': 'R', 'l3': 'V', 'r3': 'MMB',
    'precision': 'CTRL', 'select': 'TAB', 'start': 'ESC', 'walk': 'SHIFT', 'free_mouse': 'ALT',
}
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
def load(path=None):
    """{action: KEY} with the file's values over the defaults"""
    b = dict(DEFAULTS)
    path = path or os.environ.get('CONTROLS_INI') or os.path.join(ROOT, 'controls.ini')
    if os.path.isfile(path):
        for line in open(path, encoding='utf-8', errors='replace'):
            line = line.strip()
            if not line or line[0] in ';#[' or '=' not in line: continue
            k, v = line.split('=', 1); k = k.strip().lower(); v = v.strip().upper()
            if k in b and v: b[k] = v
    return b
MOUSE = {'LMB': 'L', 'RMB': 'R', 'MMB': 'M'}
ARROWS = {'UP': '↑', 'DOWN': '↓', 'LEFT': '←', 'RIGHT': '→'}
def glyph(key):
    """how the icon-font atlas draws KEY: ('mouse', side) for the three mouse buttons, else ('key', label)"""
    if key in MOUSE: return ('mouse', MOUSE[key])
    return ('key', ARROWS.get(key, {'BACKSPACE': 'BKSP', 'ENTER': 'ENTER', 'MOUSE4': 'M4', 'MOUSE5': 'M5'}.get(key, key)))
def short(key):
    """1-3 characters for a 32x32 HUD button, ' ' = the space-bar glyph"""
    if key == 'SPACE': return ' '
    return {'ENTER': 'ENT', 'BACKSPACE': 'BKS', 'SHIFT': 'SHF', 'CTRL': 'CTL', 'ALT': 'ALT', 'MOUSE4': 'M4', 'MOUSE5': 'M5'}.get(key, ARROWS.get(key, key[:3]))
