import sys, os
sys.path.insert(0, sys.argv[1])  # KFX-Input dir
from kfxlib import YJ_Book, YJ_Metadata, set_logger
import logging
logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
set_logger(logging.getLogger("kfxlib"))
src, dst, cde, asin = sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
book = YJ_Book(src)
md = YJ_Metadata()
md.cde_content_type = cde
md.asin = asin
book.decode_book(set_metadata=md, set_approximate_pages=0)
data = book.convert_to_single_kfx()
open(dst, "wb").write(data)
print("wrote", dst, len(data), "bytes")
# summarize fragment types
from collections import Counter
c = Counter(f.ftype for f in book.fragments)
for k, v in sorted(c.items()): print(k, v)
