export function parseOsc7Cwd(data: string): string | null {
  if (!data.startsWith('file://')) return null;

  const pathStart = data.indexOf('/', 'file://'.length);
  if (pathStart < 0) return null;

  try {
    const path = decodeURIComponent(data.slice(pathStart));
    if (/^\/[A-Za-z]:[\\/]/.test(path)) return path.slice(1);
    return path || null;
  } catch {
    return null;
  }
}

