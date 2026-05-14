import type { TescellateApi } from '../electron/preload';

declare global {
  interface Window {
    tescellate: TescellateApi;
  }
}

export {};
