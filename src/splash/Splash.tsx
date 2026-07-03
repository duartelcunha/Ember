import { motion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import iconUrl from "../../src-tauri/icons/128x128.png";

export default function Splash() {
  const isMac = navigator.userAgent.toLowerCase().includes("mac");
  
  // Windows: bottom-right (tray area)
  // macOS: top-right (menu bar)
  const x = isMac ? "45vw" : "45vw";
  const y = isMac ? "-45vh" : "45vh";

  const isQuit = window.location.search.includes("mode=quit");
  const isStartup = window.location.search.includes("mode=startup");

  if (isStartup) {
    return (
      <div className="w-screen h-screen flex items-center justify-center overflow-hidden bg-transparent">
        <motion.img
          src={iconUrl}
          alt="Ember Logo"
          initial={{ opacity: 0, scale: 0.8, filter: "blur(10px)" }}
          animate={{ 
            opacity: [0, 1, 1, 0], 
            scale: [0.8, 1.1, 1, 0.9],
            filter: ["blur(10px)", "blur(0px)", "blur(0px)", "blur(10px)"]
          }}
          transition={{ 
            duration: 1.2, 
            times: [0, 0.3, 0.7, 1],
            ease: "easeInOut" 
          }}
          onAnimationComplete={() => {
            invoke("close_splash").catch(console.error);
          }}
          className="w-32 h-32 drop-shadow-2xl"
        />
      </div>
    );
  }

  if (isQuit) {
    return (
      <div className="w-screen h-screen flex items-center justify-center overflow-hidden bg-transparent">
        <motion.img
          src={iconUrl}
          alt="Ember Logo"
          initial={{ opacity: 1, scale: 1 }}
          animate={{ 
            opacity: [1, 1, 0], 
            scale: [1, 1.3, 0], 
            rotate: [0, 45, 180] 
          }}
          transition={{ 
            duration: 0.6, 
            times: [0, 0.4, 1],
            ease: ["easeInOut", "easeIn"] 
          }}
          onAnimationComplete={() => {
            invoke("close_splash").catch(console.error);
          }}
          className="w-32 h-32 drop-shadow-2xl"
        />
      </div>
    );
  }

  return (
    <div className="w-screen h-screen flex items-center justify-center overflow-hidden bg-transparent">
      <motion.img
        src={iconUrl}
        alt="Ember Logo"
        initial={{ opacity: 0, scale: 0.5, x: 0, y: 0, rotate: -180 }}
        animate={{ 
          opacity: [0, 1, 1, 0], 
          scale: [0.5, 1.2, 1, 0.1], 
          x: [0, 0, 0, x],
          y: [0, 0, 0, y],
          rotate: [-180, 0, 360, 1080]
        }}
        transition={{ 
          duration: 1.8, 
          times: [0, 0.15, 0.5, 1],
          ease: ["easeOut", "easeInOut", "easeIn"] 
        }}
        onAnimationComplete={() => {
          invoke("close_splash").catch(console.error);
        }}
        className="w-32 h-32 drop-shadow-2xl"
      />
    </div>
  );
}
