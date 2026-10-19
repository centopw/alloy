import { describe, expect, it } from 'vitest';
import { formatAccuracy, formatCoordinates, mapUrl, parseGeoUri, toGeoUri } from './geo';

describe('parseGeoUri', () => {
  it('reads coordinates and the uncertainty parameter', () => {
    expect(parseGeoUri('geo:48.2,16.37;u=20')).toEqual({ latitude: 48.2, longitude: 16.37, accuracy: 20 });
    expect(parseGeoUri('geo:-33.86,151.2')).toEqual({ latitude: -33.86, longitude: 151.2, accuracy: null });
  });

  it('accepts an altitude and other parameters', () => {
    expect(parseGeoUri('geo:1,2,300;crs=wgs84;u=5')).toEqual({ latitude: 1, longitude: 2, accuracy: 5 });
  });

  it('rejects other schemes and impossible positions', () => {
    expect(parseGeoUri('https://x.org')).toBeNull();
    expect(parseGeoUri('geo:91,0')).toBeNull();
    expect(parseGeoUri('geo:0,181')).toBeNull();
    expect(parseGeoUri('geo:abc,def')).toBeNull();
  });
});

describe('formatting', () => {
  it('shows six decimals and a readable accuracy', () => {
    expect(formatCoordinates({ latitude: 48.2, longitude: 16.37 })).toBe('48.200000, 16.370000');
    expect(formatAccuracy(19.6)).toBe('about 20 m');
    expect(formatAccuracy(1500)).toBe('about 1.5 km');
  });

  it('builds a map link and a geo uri', () => {
    expect(mapUrl({ latitude: 1.5, longitude: -2 })).toBe('https://www.openstreetmap.org/?mlat=1.5&mlon=-2#map=16/1.5/-2');
    expect(toGeoUri(1.5, -2, 12.4)).toBe('geo:1.5,-2;u=12');
    expect(toGeoUri(1.5, -2)).toBe('geo:1.5,-2');
  });
});
