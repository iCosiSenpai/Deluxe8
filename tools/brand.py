"""Deluxe8 brand art: the app mark, the README banner and the GitHub social preview, as self-contained SVGs (text is
converted to paths with the launcher's own font, Archivo, OFL, so the files render anywhere).

    pip install fonttools uharfbuzz
    python tools/brand.py [out dir]          # default: docs/assets

Then: copy logo.svg to apps/launcher/src-tauri/icons/source.svg, render it at 1024x1024 (any browser:
`chrome --headless=new --screenshot=icon.png --window-size=1024,1024 --default-background-color=00000000 logo.svg`) and
run `pnpm tauri icon icon.png` in apps/launcher; render social-preview.svg at 1280x640 for the repository settings.
"""
import sys
from pathlib import Path

import uharfbuzz as hb
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[1]
FONTS = ROOT / 'apps' / 'launcher' / 'src' / 'assets' / 'fonts'
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'docs' / 'assets'
OUT.mkdir(parents=True, exist_ok=True)

# "Pit wall at night"
ASFALTO, ASFALTO_2, PANNELLO, PANNELLO_2 = '#161b26', '#1a2030', '#1e2433', '#252c3e'
LINEA, LINEA_FORTE = '#2c3447', '#3a4460'
GESSO, GESSO_2, GESSO_3 = '#f2efe8', '#b9bcc6', '#7d8396'
VIOLA, VERDE, GIALLO, ROSSO = '#9b5cff', '#2ed47a', '#ffd23f', '#ff3b30'
CORDOLO = '#e0362c'

_cache = {}


def _font(italic, wght, wdth):
    key = (italic, wght, wdth)
    if key not in _cache:
        path = FONTS / ('Archivo-Italic-Variable.ttf' if italic else 'Archivo-Variable.ttf')
        tt = instancer.instantiateVariableFont(TTFont(str(path)), {'wght': wght, 'wdth': wdth})
        face = hb.Face(hb.Blob.from_file_path(str(path)))
        font = hb.Font(face)
        font.set_variations({'wght': wght, 'wdth': wdth})
        _cache[key] = (tt, font)
    return _cache[key]


def text(s, size, x=0.0, y=0.0, italic=True, wght=900, wdth=125, tracking=0.0, anchor='start'):
    """SVG path data for `s` with its baseline at y; returns (d, width, (xmin, ymin, xmax, ymax))."""
    tt, font = _font(italic, wght, wdth)
    buf = hb.Buffer()
    buf.add_str(s)
    buf.guess_segment_properties()
    hb.shape(font, buf, {'kern': True, 'liga': True})
    upem = tt['head'].unitsPerEm
    k = size / upem
    order = tt.getGlyphOrder()
    gs = tt.getGlyphSet()
    track = tracking * upem
    total = sum(p.x_advance for p in buf.glyph_positions) + track * (len(buf.glyph_positions) - 1)
    if anchor == 'middle':
        x -= total * k / 2
    elif anchor == 'end':
        x -= total * k
    pen = SVGPathPen(gs)
    bounds = BoundsPen(gs)
    pos_x = 0.0
    for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
        name = order[info.codepoint]
        m = (k, 0, 0, -k, x + (pos_x + pos.x_offset) * k, y - pos.y_offset * k)
        gs[name].draw(TransformPen(pen, m))
        gs[name].draw(TransformPen(bounds, m))
        pos_x += pos.x_advance + track
    return pen.getCommands(), total * k, bounds.bounds


def lights(cx0, cy, r, gap, n=5, glow=True):
    out = []
    for i in range(n):
        cx = cx0 + i * gap
        if glow:
            out.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r * 1.9:.1f}" fill="{ROSSO}" opacity="0.22" filter="url(#blur)"/>')
        out.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r:.1f}" fill="{ROSSO}"/>')
        out.append(f'<circle cx="{cx - r * 0.32:.1f}" cy="{cy - r * 0.32:.1f}" r="{r * 0.28:.1f}" fill="#ffffff" opacity="0.35"/>')
    return '\n'.join(out)


def kerb(x, y, w, h, stripe, skew):
    """Red/white kerb band, stripes slanted like the italic type."""
    parts = [f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{GESSO}"/>']
    sx = x - h
    while sx < x + w:
        parts.append(
            f'<path d="M{sx:.1f} {y + h} L{sx + skew:.1f} {y} h{stripe:.1f} L{sx + stripe:.1f} {y + h} Z" fill="{CORDOLO}"/>'
        )
        sx += stripe * 2
    return '\n'.join(parts)


def mark(size, ox=0.0, oy=0.0, radius_ratio=0.22, shadow=False):
    """The app mark: a race-number plate with the 8, five start lights and a kerb, on night asphalt."""
    s = size / 1024
    g = [f'<g transform="translate({ox:.1f} {oy:.1f}) scale({s:.5f})">']
    if shadow:
        g.append(f'<rect x="10" y="26" width="1004" height="1004" rx="{1024 * radius_ratio:.0f}" fill="#000" opacity="0.35" filter="url(#blur-big)"/>')
    g.append(f'<rect width="1024" height="1024" rx="{1024 * radius_ratio:.0f}" fill="url(#asphalt)"/>')
    g.append(f'<rect x="3" y="3" width="1018" height="1018" rx="{1024 * radius_ratio - 3:.0f}" fill="none" stroke="{LINEA_FORTE}" stroke-width="6" opacity="0.7"/>')
    g.append(lights(232, 196, 46, 140))
    # the plate: a parallelogram slanted like Archivo's italic, with the number in asphalt
    g.append(f'<path d="M262 318 H866 L790 770 H186 Z" fill="{GESSO}"/>')
    d, w, b = text('8', 470, 0, 0, italic=True, wght=900, wdth=125)
    gx = 526 - (b[0] + b[2]) / 2
    gy = 544 - (b[1] + b[3]) / 2
    g.append(f'<path transform="translate({gx:.1f} {gy:.1f})" d="{d}" fill="{ASFALTO}"/>')
    g.append('<clipPath id="plate-kerb"><path d="M178 820 H850 L838 880 H166 Z"/></clipPath>')
    g.append(f'<g clip-path="url(#plate-kerb)">{kerb(150, 820, 720, 60, 56, 14)}</g>')
    g.append('</g>')
    return '\n'.join(g)


DEFS = f'''<defs>
  <linearGradient id="asphalt" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="{PANNELLO_2}"/>
    <stop offset="1" stop-color="{ASFALTO}"/>
  </linearGradient>
  <linearGradient id="night" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="{PANNELLO}"/>
    <stop offset="0.55" stop-color="{ASFALTO_2}"/>
    <stop offset="1" stop-color="{ASFALTO}"/>
  </linearGradient>
  <radialGradient id="flood" cx="0.78" cy="0.05" r="0.75">
    <stop offset="0" stop-color="{ROSSO}" stop-opacity="0.16"/>
    <stop offset="1" stop-color="{ROSSO}" stop-opacity="0"/>
  </radialGradient>
  <filter id="blur" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="9"/></filter>
  <filter id="blur-big" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="22"/></filter>
</defs>'''


def svg(w, h, body, title):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" '
            f'aria-label="{title}">\n<title>{title}</title>\n{DEFS}\n{body}\n</svg>\n')


def chips(labels, x, y, size, height, gap, colour_dot=None):
    out, cx = [], x
    for label in labels:
        d, w, _ = text(label, size, 0, 0, italic=False, wght=650, wdth=112, tracking=0.04)
        pad = height * 0.55
        dot = height * 0.42 if colour_dot else 0
        cw = w + pad * 2 + dot
        out.append(f'<rect x="{cx:.1f}" y="{y:.1f}" width="{cw:.1f}" height="{height}" rx="{height / 2:.1f}" '
                   f'fill="{PANNELLO_2}" stroke="{LINEA_FORTE}" stroke-width="1.5"/>')
        if colour_dot:
            out.append(f'<circle cx="{cx + pad + height * 0.12:.1f}" cy="{y + height / 2:.1f}" r="{height * 0.12:.1f}" fill="{colour_dot}"/>')
        tx = cx + pad + dot
        ty = y + height / 2 + size * 0.36
        out.append(f'<path transform="translate({tx:.1f} {ty:.1f})" d="{d}" fill="{GESSO}"/>')
        cx += cw + gap
    return '\n'.join(out), cx - gap - x


def hero(w, h, *, mark_size, mark_x, word_size, tag_size, chip_size, chip_h, pad, lights_y, kerb_h, rx, title):
    body = [f'<rect width="{w}" height="{h}" rx="{rx}" fill="url(#night)"/>',
            f'<rect width="{w}" height="{h}" rx="{rx}" fill="url(#flood)"/>']
    # racing line
    body.append(f'<path d="M{-20} {h * 0.86:.1f} C {w * 0.3:.1f} {h * 0.6:.1f}, {w * 0.62:.1f} {h * 1.02:.1f}, {w + 20} {h * 0.58:.1f}" '
                f'fill="none" stroke="{LINEA_FORTE}" stroke-width="2" stroke-dasharray="2 14" stroke-linecap="round" opacity="0.8"/>')
    my = (h - kerb_h - mark_size) / 2 - h * 0.02
    body.append(mark(mark_size, mark_x, my, shadow=True))
    tx = mark_x + mark_size + pad
    avail = w - tx - pad
    d, ww, b = text('DELUXE8', word_size, 0, 0, italic=True, wght=900, wdth=125, tracking=-0.01)
    scale = min(1.0, avail / (b[2] - b[0]))
    word_h = (b[3] - b[1]) * scale
    tag_d, tag_w, tb = text('Your own copy, tuned for PC.', tag_size, 0, 0, italic=False, wght=500, wdth=100)
    block = word_h + tag_size * 0.9 + tag_size * 1.25 + chip_h
    top = my + (mark_size - block) / 2
    base = top + (-b[1]) * scale
    body.append(f'<path transform="translate({tx - b[0] * scale:.1f} {base:.1f}) scale({scale:.4f})" d="{d}" fill="{GESSO}"/>')
    ty = top + word_h + tag_size * 1.15
    body.append(f'<path transform="translate({tx:.1f} {ty:.1f})" d="{tag_d}" fill="{GESSO_2}"/>')
    labels = ['DLSS', 'FSR 3.1', 'FRAME GEN', 'HDR10', '4K', 'ONLINE ROOMS']
    cy = ty + tag_size * 0.75
    row, roww = chips(labels, tx, cy, chip_size, chip_h, chip_h * 0.32)
    if roww > avail:  # fewer chips on narrow canvases
        row, roww = chips(labels[:5], tx, cy, chip_size, chip_h, chip_h * 0.32)
    body.append(row)
    body.append(f'<clipPath id="card"><rect width="{w}" height="{h}" rx="{rx}"/></clipPath>')
    body.append(f'<g clip-path="url(#card)">{kerb(0, h - kerb_h, w, kerb_h, kerb_h * 1.4, kerb_h * 0.35)}</g>')
    return svg(w, h, '\n'.join(body), title)


icon = svg(1024, 1024, mark(1024), 'Deluxe8')
(OUT / 'logo.svg').write_text(icon, encoding='utf-8')

banner = hero(1600, 460, mark_size=300, mark_x=96, word_size=176, tag_size=40, chip_size=21, chip_h=44, pad=64,
              lights_y=0, kerb_h=24, rx=28, title='Deluxe8: your own copy, tuned for PC')
(OUT / 'banner.svg').write_text(banner, encoding='utf-8')

social = hero(1280, 640, mark_size=300, mark_x=80, word_size=150, tag_size=38, chip_size=19, chip_h=40, pad=56,
              lights_y=0, kerb_h=30, rx=0, title='Deluxe8')
(OUT / 'social-preview.svg').write_text(social, encoding='utf-8')

for f in sorted(OUT.glob('*.svg')):
    print(f.name, f.stat().st_size)
