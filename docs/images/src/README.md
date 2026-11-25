# Diagram sources

`architecture.html` and `system-map-overview.html` are the sources for the PNGs in
`docs/images/`. They were made with the
[stack-diagram](https://github.com/caru-ini/stack-diagram) skill. To change a
diagram, edit its HTML here and rebuild it. Do not edit the PNGs.

```bash
# from the stack-diagram skill folder
python scripts/fetch_icons.py <icons-dir> $(cat <repo>/docs/images/src/icons.txt)
python scripts/build.py <repo>/docs/images/src/architecture.html <icons-dir> out.html --fonts assets/fonts
python scripts/render_png.py out.html <repo>/docs/images/architecture.png --selector .canvas --scale 2
```

`render_png.py` writes both the light PNG and the `-dark` PNG.
