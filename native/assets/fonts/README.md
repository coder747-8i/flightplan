Inter 4.1 (SIL Open Font License, see Inter-LICENSE.txt), static TTFs from the release zip.
The private-use-area code points are removed so Phosphor's icons (which live there) aren't
shadowed by Inter's alternate glyphs:

    pyftsubset Inter-Regular.ttf --unicodes="U+0000-D7FF,U+F900-FFFD,U+10000-10FFFF" \
      --layout-features='*' --name-IDs='*' --notdef-outline --output-file=Inter-Regular.ttf
