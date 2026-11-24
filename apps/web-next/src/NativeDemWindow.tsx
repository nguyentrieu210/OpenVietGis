import {useEffect, useRef, useState} from "react";
import type {GeoTIFFImage} from "geotiff";

const windowSide = 256;

export function NativeDemWindow({url}: {url: string}) {
    const [image, setImage] = useState<GeoTIFFImage|null>(null);
    const [origin, setOrigin] = useState({x:0, y:0});
    const [status, setStatus] = useState("");
    const [error, setError] = useState("");
    const canvas = useRef<HTMLCanvasElement>(null);

    useEffect(() => {
        const controller = new AbortController();
        async function open() {
            setStatus("Đang đọc thông tin COG…");
            try {
                const {fromUrl} = await import("geotiff");
                const options = {allowFullFile:false, blockSize:65536, cacheSize:64};
                const file = await fromUrl(url, options, controller.signal);
                const native = await file.getImage(0);
                if (!controller.signal.aborted) setImage(native);
            } catch (err) {
                if (!controller.signal.aborted) {
                    setStatus("");
                    setError(err instanceof Error ? err.message : String(err));
                }
            }
        }
        void open();
        return () => controller.abort();
    }, [url]);

    useEffect(() => {
        if (!image) return;
        const source = image;
        const controller = new AbortController();
        async function read() {
            setError("");
            setStatus("Đang đọc pixel gốc…");
            const context = canvas.current?.getContext("2d");
            if (!context) return;
            context.clearRect(0,0,windowSide,windowSide);
            try {
                const {x,y} = origin;
                const right = Math.min(x + windowSide, source.getWidth());
                const bottom = Math.min(y + windowSide, source.getHeight());
                const width = right - x, height = bottom - y;
                const raster = await source.readRasters({
                    window:[x,y,right,bottom], samples:[0], interleave:false, signal:controller.signal,
                });
                if (controller.signal.aborted) return;
                if (!Array.isArray(raster) || !raster[0]) throw Error("Không đọc được band cao độ.");
                const values = raster[0];
                const nodata = source.getGDALNoData();
                const valid = (value: number) => Number.isFinite(value) && value !== nodata;
                let min = Infinity, max = -Infinity, count = 0;
                for (const value of values) {
                    if (!valid(value)) continue;
                    min = Math.min(min,value);
                    max = Math.max(max,value);
                    count++;
                }
                const pixels = context.createImageData(width,height);
                values.forEach((value,i) => {
                    if (!valid(value)) return;
                    const intensity = Math.round(45 + 210 * (max === min ? .5 : (value-min)/(max-min)));
                    pixels.data.set([intensity,intensity,intensity,255],4*i);
                });
                context.putImageData(pixels,0,0);
                setStatus(count ? `${width}×${height} pixel · ${min.toFixed(2)}–${max.toFixed(2)} m · ${values.length-count} NoData` : "Cửa sổ chỉ chứa NoData");
            } catch (err) {
                if (!controller.signal.aborted) {
                    setStatus("");
                    setError(err instanceof Error ? err.message : String(err));
                }
            }
        }
        void read();
        return () => controller.abort();
    }, [image,origin]);

    function move(dx:number,dy:number) {
        if (!image) return;
        setOrigin(({x,y}) => ({
            x:Math.max(0, Math.min(x+dx, Math.floor((image.getWidth()-1)/windowSide)*windowSide)),
            y:Math.max(0, Math.min(y+dy, Math.floor((image.getHeight()-1)/windowSide)*windowSide)),
        }));
    }

    return <section aria-label="Cửa sổ DEM gốc">
        {image && <>
            <p>{image.getWidth()}×{image.getHeight()} pixel · Cột {origin.x + 1}, hàng {origin.y + 1}</p>
            <div>
                <button disabled={!origin.x} onClick={()=>move(-windowSide,0)}>Trái</button>
                <button disabled={!origin.y} onClick={()=>move(0,-windowSide)}>Lên</button>
                <button disabled={origin.y+windowSide>=image.getHeight()} onClick={()=>move(0,windowSide)}>Xuống</button>
                <button disabled={origin.x+windowSide>=image.getWidth()} onClick={()=>move(windowSide,0)}>Phải</button>
            </div>
            <canvas ref={canvas} width={windowSide} height={windowSide} style={{maxWidth:"100%", imageRendering:"pixelated"}} aria-label="Ảnh cao độ tại cửa sổ pixel gốc"/>
        </>}
        {status && <p role="status">{status}</p>}
        {error && <p role="alert">{error}</p>}
    </section>;
}
