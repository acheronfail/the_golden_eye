import OBSWebSocket from 'obs-websocket-js/msgpack';
import { settings } from './settings.svelte';

const obs = new OBSWebSocket();
let connected = false;

const connect = async () => {
	if (!connected) {
		await obs.connect(settings.obsUrl, settings.obsPassword);
		connected = true;
	}
};

export const disconnect = async () => {
	if (connected) {
		await obs.disconnect();
		connected = false;
	}
};

export const testConnection = async () => {
	let succeeded = false;
	try {
		await obs.connect(settings.obsUrl, settings.obsPassword);
		succeeded = true;
		await obs.disconnect();
	} catch (error) {
		console.error('Failed to connect to OBS:', error);
	}

	return succeeded;
};

export const getFrame = async () => {
	try {
		await connect();

		const { imageData } = await obs.call('GetSourceScreenshot', {
			sourceName: 'Capture Card Device',
			imageFormat: 'png',
			imageWidth: 1280,
			imageHeight: 720
		});

		return imageData;
	} catch (error) {
		console.error('Failed to get frame from OBS:', error);
		return null;
	}
};
