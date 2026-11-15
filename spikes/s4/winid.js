ObjC.import('CoreGraphics');
function run(argv) {
  var list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(0, 0)));
  var hits = list.filter(function (w) { return (w.kCGWindowName || '').indexOf(argv[0]) >= 0; });
  hits.sort(function (a, b) { return b.kCGWindowNumber - a.kCGWindowNumber; });
  return hits.length ? String(hits[0].kCGWindowNumber) : '';
}
