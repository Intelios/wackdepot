"""Commit Buddy geometry. Nonzero fill: outline parts wind clockwise, holes counter-clockwise."""
def squircle(x0,y0,x1,y1,a,k):
    return (f"M{x0+a} {y0} H{x1-a} C{x1-a+k} {y0} {x1} {y0+a-k} {x1} {y0+a} "
            f"V{y1-a} C{x1} {y1-a+k} {x1-a+k} {y1} {x1-a} {y1} H{x0+a} "
            f"C{x0+a-k} {y1} {x0} {y1-a+k} {x0} {y1-a} V{y0+a} C{x0} {y0+a-k} {x0+a-k} {y0} {x0+a} {y0} Z")
def pill_hole(cx,top,w,h):
    r=w/2
    return f"M{cx+r} {top+r} A{r} {r} 0 0 0 {cx-r} {top+r} V{top+h-r} A{r} {r} 0 0 0 {cx+r} {top+h-r} Z"
def sym(look=8, lw=24, a=64, k=44, ey=-34, ew=20, eh=44, gap=16, sr=22, sy=22,
        body=(52,56,204,200), line=(20,236), dx=0, dy=0):
    """ey/sy: eye top and smile top relative to the body's centre; sr: smile radius (0 = none)."""
    x0,y0,x1,y1=body[0]+dx,body[1]+dy,body[2]+dx,body[3]+dy; 
    cy=(y0+y1)/2; L,R=(line[0]+dx,line[1]+dx) if line else (0,0); cx=(x0+x1)/2+look; r=lw/2
    d=[] if line is None else [
       f"M{L+r} {cy-r} H{x0+16} V{cy+r} H{L+r} A{r} {r} 0 0 1 {L+r} {cy-r} Z",
       f"M{x1-16} {cy-r} H{R-r} A{r} {r} 0 0 1 {R-r} {cy+r} H{x1-16} Z"]
    d+=[squircle(x0,y0,x1,y1,a,k),
       pill_hole(cx-gap/2-ew/2,cy+ey,ew,eh), pill_hole(cx+gap/2+ew/2,cy+ey,ew,eh)]
    if sr:
        s=cy+sy
        d.append(f"M{cx+sr} {s} H{cx-sr} A{sr} {sr} 0 0 0 {cx+sr} {s} Z")
    return " ".join(d)
SMALL=dict(look=4,lw=36,ew=32,eh=48,gap=28,ey=-46,sr=34,sy=16,body=(40,44,216,212),line=(4,252),a=68,k=46)
def svg(d,title,fill="#000",vb="0 0 256 256",w=256,h=256,extra=""):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" width="{w}" height="{h}" role="img" aria-labelledby="title">'
            f'<title id="title">{title}</title>{extra}<path id="symbol" fill="{fill}" d="{d}"/></svg>\n')
