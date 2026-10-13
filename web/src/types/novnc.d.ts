// @types/novnc__novnc describes the old `lib/rfb` entry point; noVNC 1.7 ships the same class as
// its package main (`core/rfb.js`).
declare module "@novnc/novnc" {
  import RFB from "@novnc/novnc/lib/rfb";
  export default RFB;
}
