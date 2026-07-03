import { motion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import iconUrl from "../../src-tauri/icons/128x128.png";

export default function Splash() {
  const isMac = navigator.userAgent.toLowerCase().includes("mac");
  
  // Windows: bottom-right (tray area)
  // macOS: top-right (menu bar)
  const x = isMac ? "45vw" : "45vw";
  const y = isMac ? "-45vh" : "45vh";

  return (
    <div className="w-screen h-screen flex items-center justify-center overflow-hidden bg-transparent">
      <motion.img
        src={iconUrl}
        alt="Ember Logo"
        initial={{ opacity: 0, scale: 0.5, x: 0, y: 0 }}
        animate={{ 
          opacity: [0, 1, 1, 0], 
          scale: [0.5, 1.2, 1, 0.1], 
          x: [0, 0, 0, x],
          y: [0, 0, 0, y] 
        }}
        transition={{ 
          duration: 1.5, 
          times: [0, 0.2, 0.6, 1],
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
