import sys, logging
sys.path.insert(0, sys.argv[1])
from kfxlib import YJ_Book, YJ_Metadata, set_logger
logging.basicConfig(level=logging.WARNING); set_logger(logging.getLogger("kfxlib"))
src, dst, cde, asin, pages = sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6]
book = YJ_Book(src); md = YJ_Metadata(); md.cde_content_type = cde; md.asin = asin
book.decode_book(set_metadata=md, set_approximate_pages=None if pages == "none" else int(pages))
data = book.convert_to_single_kfx(); open(dst, "wb").write(data); print("wrote", dst, len(data))
