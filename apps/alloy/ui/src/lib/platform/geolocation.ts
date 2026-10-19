/** Readable text for a failed position request (`GeolocationPositionError.code`). */
export function locationErrorMessage(code: number): string {
  if (code === 1) return 'Location access is turned off for Alloy. Allow it in your system settings and try again.';
  if (code === 2) return 'Your position could not be determined. Check that location services are on.';
  if (code === 3) return 'Finding your position took too long. Try again outdoors or with Wi-Fi on.';
  return 'Your position could not be read.';
}

/** Whether this runtime can report a position. */
export function canLocate(): boolean {
  return typeof navigator !== 'undefined' && 'geolocation' in navigator;
}

export interface Fix {
  latitude: number;
  longitude: number;
  /** Radius of uncertainty in metres. */
  accuracy: number;
}

function toFix(position: GeolocationPosition): Fix {
  const { latitude, longitude, accuracy } = position.coords;
  return { latitude, longitude, accuracy };
}

/** One high-accuracy fix, or a rejection with a readable message. */
export function currentFix(timeoutMs = 10_000): Promise<Fix> {
  const { promise, resolve, reject } = Promise.withResolvers<Fix>();
  if (!canLocate()) {
    reject(new Error('This device cannot report a position.'));
    return promise;
  }
  navigator.geolocation.getCurrentPosition(
    (position) => resolve(toFix(position)),
    (error) => reject(new Error(locationErrorMessage(error.code))),
    { enableHighAccuracy: true, timeout: timeoutMs, maximumAge: 0 },
  );
  return promise;
}

/** Follows the device position; returns a function that stops watching. */
export function watchFix(onfix: (fix: Fix) => void, onerror: (message: string) => void): () => void {
  if (!canLocate()) {
    onerror('This device cannot report a position.');
    return () => {};
  }
  const id = navigator.geolocation.watchPosition(
    (position) => onfix(toFix(position)),
    (error) => onerror(locationErrorMessage(error.code)),
    { enableHighAccuracy: true, maximumAge: 5_000 },
  );
  return () => navigator.geolocation.clearWatch(id);
}
