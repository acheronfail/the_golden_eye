import { OBSWebSocketError } from 'obs-websocket-js';

export const ObsErrorCode = {
  connectFail:
    'Failed to connect to OBS WebSocket, please make sure OBS is running and the WebSocket setting is enabled.',
  badPassword: 'Incorrect OBS password, please check OBS_PASSWORD is set correctly.',
  noSource: 'Failed to find video source, please ensure OBS_SOURCE_NAME is set correctly.',
  unknown: 'An unknown error occurred when communicating with OBS.',
} as const;

export class ObsError extends Error {
  public readonly cause?: unknown;
  constructor(message: string, cause?: unknown) {
    super(message);
    this.name = 'ObsError';
    this.cause = cause;
  }

  static catch = (cause: unknown) => {
    if (cause instanceof OBSWebSocketError) {
      switch (cause.code) {
        case 600:
          throw new ObsError(ObsErrorCode['noSource'], cause);
        case 501: // replay buffer was off, and we tried to turn it off
        case 604: // replay buffer is not enabled, and we tried to query its status, just swallow
          return null;
        case 1006:
          throw new ObsError(ObsErrorCode['connectFail'], cause);
        case 4009:
          throw new ObsError(ObsErrorCode['badPassword'], cause);
      }
    }

    throw new ObsError(ObsErrorCode['unknown'], cause);
  };
}
