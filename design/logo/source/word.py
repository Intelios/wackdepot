import uharfbuzz as hb
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.boundsPen import BoundsPen
import os
FONT=os.path.join(os.path.dirname(os.path.abspath(__file__)),"../../../assets/fonts/Inter-Bold.ttf")
def outline(text, size, x, baseline, track=0.0):
    """Shaped (GPOS kerning) Inter Bold outlines. Returns path d, ink bounds, cap height, advance width."""
    f=TTFont(FONT); gs=f.getGlyphSet(); order=f.getGlyphOrder(); upm=f["head"].unitsPerEm
    blob=hb.Blob.from_file_path(FONT); font=hb.Font(hb.Face(blob))
    buf=hb.Buffer(); buf.add_str(text); buf.guess_segment_properties(); hb.shape(font,buf,{"kern":True,"liga":False})
    sc=size/upm; pen=SVGPathPen(gs); bp=BoundsPen(gs); cur=0
    last=len(buf.glyph_infos)-1
    for i,(info,pos) in enumerate(zip(buf.glyph_infos,buf.glyph_positions)):
        n=order[info.codepoint]; m=(sc,0,0,-sc,x+(cur+pos.x_offset)*sc,baseline)
        gs[n].draw(TransformPen(pen,m)); gs[n].draw(TransformPen(bp,m))
        cur+=pos.x_advance+(track*upm if i<last else 0)  # tracking goes between glyphs only
    return pen.getCommands(), bp.bounds, f["OS/2"].sCapHeight*sc, cur*sc
