# Test files of the extraction

Small files for the tests of `strata-extract` and of the worker (change 3 of #49). Each file holds no real content. Ghostscript made the PDF files from the PostScript files in this folder:

| File | Command or source |
|---|---|
| `two-pages.pdf` | `gs -q -dNOPAUSE -dBATCH -sDEVICE=pdfwrite -o two-pages.pdf two-pages.ps` |
| `scan-no-text.pdf` | `gs -q -dNOPAUSE -dBATCH -sDEVICE=pdfwrite -o scan-no-text.pdf scan-no-text.ps` (a gray box, with no text) |
| `user-password.pdf` | The command of `two-pages.pdf`, with `-sOwnerPassword=owner1 -sUserPassword=user1` |
| `owner-password.pdf` | The command of `two-pages.pdf`, with `-sOwnerPassword=owner1 -dPermissions=-4` |
| `broken.pdf` | A file that starts with `%PDF-1.7` and has no valid structure |
| `report.pdf` | A ZIP archive with one file `a.txt`, from Python `zipfile` |
