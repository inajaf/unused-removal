# Windows marketing presentation

The GIF uses the production HTML, CSS, localization, and rendering code. `render_demo.js` intercepts the app's internal API in Playwright, so the capture needs no HTTP server, localhost listener, or open port. Its mock data exists only to make the same scan states reproducible for marketing.

## Rebuild

```powershell
node marketing/render_demo.js
python marketing/build_presentation.py
ffmpeg -y -framerate 8 -i marketing/.frames/frame_%03d.png -filter_complex "[0:v]split[a][b];[a]palettegen=max_colors=96:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" -loop 0 marketing/dist/unused-removal-windows-demo.gif
```

The final GIF is 960 × 600, 8 fps, and uses a 96-color adaptive palette to keep it compact.

## Comparison sources

- [WinDirStat](https://windirstat.net/)
- [TreeSize features](https://www.jam-software.com/treesize/features.shtml) and [editions](https://www.jam-software.com/treesize/editions.shtml)
- [BleachBit features](https://www.bleachbit.org/features) and [downloads](https://www.bleachbit.org/download)

The comparison describes each product's documented primary workflow. It deliberately avoids unsupported speed or "best in market" claims.

