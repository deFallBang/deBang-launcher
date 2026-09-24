import { convertFileSrc } from "@tauri-apps/api/core";
import { useApp } from "../state/app";

export function Background() {
  const { settings } = useApp();
  const src = settings.bgPath ? convertFileSrc(settings.bgPath) : "";
  const isVideo = /\.(webm|mp4|mkv|ogg)$/i.test(settings.bgPath);

  return (
    <>
      {settings.bgType !== "gradient" && src && (
        settings.bgType === "video" && isVideo ? (
          <video
            className="bg-media"
            src={src}
            autoPlay
            loop
            muted
            playsInline
          />
        ) : (
          <img className="bg-media" src={src} alt="" />
        )
      )}
      <div className="bg-aurora" />
      <div className="bg-dim" />
    </>
  );
}
