export interface GeoPoint {
  latitude: number;
  longitude: number;
  /** Uncertainty radius in metres, when the sender gave one. */
  accuracy: number | null;
}

/** Parses an RFC 5870 `geo:` URI (`geo:48.2,16.37;u=20`). Returns null for anything else or out-of-range values. */
export function parseGeoUri(uri: string): GeoPoint | null {
  const match = /^geo:(-?\d+(?:\.\d+)?),(-?\d+(?:\.\d+)?)(?:,-?\d+(?:\.\d+)?)?((?:;[^;]*)*)$/i.exec(uri.trim());
  if (!match) return null;
  const latitude = Number(match[1]);
  const longitude = Number(match[2]);
  if (Math.abs(latitude) > 90 || Math.abs(longitude) > 180) return null;
  const u = /;u=(\d+(?:\.\d+)?)/i.exec(match[3]);
  return { latitude, longitude, accuracy: u ? Number(u[1]) : null };
}

/** "48.200000, 16.370000" with six decimals (about 10 cm). */
export function formatCoordinates(point: Pick<GeoPoint, 'latitude' | 'longitude'>): string {
  return `${point.latitude.toFixed(6)}, ${point.longitude.toFixed(6)}`;
}

/** "about 20 m" / "about 1.5 km" for an accuracy radius. */
export function formatAccuracy(metres: number): string {
  return metres >= 1000 ? `about ${(metres / 1000).toFixed(1)} km` : `about ${Math.round(metres)} m`;
}

/** A map page for the point. No tiles are loaded in the app, so nothing leaves the device until the user opens this. */
export function mapUrl(point: Pick<GeoPoint, 'latitude' | 'longitude'>): string {
  const { latitude, longitude } = point;
  return `https://www.openstreetmap.org/?mlat=${latitude}&mlon=${longitude}#map=16/${latitude}/${longitude}`;
}

/** Builds the `geo:` URI the engine sends for live location beacons. */
export function toGeoUri(latitude: number, longitude: number, accuracy?: number | null): string {
  const u = accuracy != null && Number.isFinite(accuracy) && accuracy >= 0 ? `;u=${Math.round(accuracy)}` : '';
  return `geo:${latitude},${longitude}${u}`;
}
