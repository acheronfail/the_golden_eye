/**
 * Much inspiration taken from: https://github.com/jakzo/NeonWhiteMods/blob/main/scripts/upload-to-youtube.ts
 */

import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { readdir, readFile, stat, writeFile } from 'node:fs/promises';
import { google } from 'googleapis';
import { readEnv } from '../obs/envfile.ts';
import { checkbox } from '@inquirer/prompts';
import { parseVideoName } from '../obs/naming.ts';

//
// Setup
//

await readEnv();

const __dirname = dirname(fileURLToPath(import.meta.url));

const playlistTitle = process.env.PLAYLIST_TITLE;
if (!playlistTitle) {
  console.error('PLAYLIST_TITLE environment variable is not set');
  process.exit(1);
}

const [, , videoDir] = process.argv;
if (!videoDir || process.argv.length !== 3) {
  console.error('Usage: just upload <videoDir>');
  process.exit(1);
}

//
// Prompt for which videos to upload
//

const videos = await readdir(videoDir).then((files) => files.filter((name) => name.endsWith('.mp4')));
if (!videos.length) {
  console.error('No videos found in directory:', videoDir);
  process.exit(1);
}

const videosToUpload = await checkbox({
  message: 'Select videos to upload',
  choices: videos.map((video) => ({ name: video, value: video })),
}).then((names) => names.map((name) => join(videoDir, name)));

if (!videosToUpload.length) {
  console.error('No videos selected for upload');
  process.exit(0);
}

//
// OAuth Server
//

const tokenPath = join(__dirname, 'generated-tokens.json');

let oauthCodeResolve: (oauthCode: string) => void;
let oauthCodeReject: (err: unknown) => void;
const oauthCode = new Promise<string>((resolve, reject) => {
  oauthCodeResolve = resolve;
  oauthCodeReject = reject;
});

const server = createServer((req, res) => {
  try {
    if (!req.url) {
      res.statusCode = 400;
      res.end('Bad Request');
      return;
    }

    const url = new URL(req.url, 'http://localhost');
    const code = url.searchParams.get('code');
    if (!code) {
      return new Response('No OAuth2 code provided', { status: 400 });
    }

    oauthCodeResolve(code);

    res.statusCode = 200;
    res.setHeader('Content-Type', 'text/html');
    res.end('<html><body>You can now close this window. <script>window.close();</script></body></html>');
  } catch (err) {
    console.error('Error handling request:', err);
    oauthCodeReject(err);
    res.statusCode = 500;
    res.end('Internal Server Error');
  }
}).listen(0);

await new Promise((resolve) => server.on('listening', resolve));
const addr = server.address();
const port = typeof addr === 'object' && addr !== null ? addr.port : 0;

const credentials = JSON.parse(await readFile(join(__dirname, 'credentials.json'), 'utf-8'));
const { client_secret, client_id } = credentials.installed;
const oauth2Client = new google.auth.OAuth2(client_id, client_secret, `http://localhost:${port}`);

if (await stat(tokenPath)) {
  oauth2Client.setCredentials(JSON.parse(await readFile(tokenPath, 'utf-8')));
} else {
  const authorizationUrl = oauth2Client.generateAuthUrl({
    access_type: 'offline',
    scope: ['https://www.googleapis.com/auth/youtube', 'https://www.googleapis.com/auth/youtube.upload'],
  });

  open(authorizationUrl);

  const { tokens } = await oauth2Client.getToken(await oauthCode);
  await writeFile(tokenPath, JSON.stringify(tokens));

  oauth2Client.setCredentials(tokens);

  server.close();
  server.closeAllConnections();
  await new Promise((resolve) => server.on('close', resolve));
}

//
// Read Playlist
//

const youtube = google.youtube({ version: 'v3', auth: oauth2Client });

console.log('Finding playlist...');
youtube;
const { data: existingPlaylists } = await youtube.playlists.list({
  part: ['snippet'],
  mine: true,
});

let playlist = existingPlaylists.items?.find((item) => item?.snippet?.title === playlistTitle);
if (playlist) {
  console.log('Playlist found');
} else {
  console.log('Playlist not found, creating...');
  ({ data: playlist } = await youtube.playlists.insert({
    part: ['snippet', 'status'],
    requestBody: {
      snippet: {
        title: playlistTitle,
        description: 'Goldeneye speedruns',
      },
      status: {
        privacyStatus: 'unlisted',
      },
    },
  }));
  console.log('Created new playlist with ID:', playlist.id);
}

const existingPlaylistItems: {
  id: string;
  title: string;
  levelName: string | undefined;
}[] = [];

let nextPageToken: string | undefined = undefined;
do {
  const { data: playlistItems } = await youtube.playlistItems.list({
    part: ['snippet'],
    playlistId: playlist.id,
    pageToken: nextPageToken,
  });

  for (const item of playlistItems.items ?? []) {
    existingPlaylistItems.push({
      id: item.id,
      title: item.snippet.title,
      levelName: item.snippet.title.match(/\] (.+?) - /)?.[1],
    });
  }

  nextPageToken = playlistItems.nextPageToken;
} while (nextPageToken);

console.log('Existing playlist items:', existingPlaylistItems);

//
// Upload Videos
//

for (const videoPath of videosToUpload) {
  const videoFileName = basename(videoPath, '.mp4');
  const nameParts = parseVideoName(videoFileName);
  if (!nameParts) {
    console.warn(`Skipping video with unrecognized name format: ${videoFileName}`);
    continue;
  }

  const { levelNumber, level, difficulty, time, date } = nameParts;
  const title = [levelNumber, level, difficulty, time].join(' - ');
  const description = `Date achieved: ${date.toLocaleString([], {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })}`;

  // TODO: upload video
  // TODO: add video at right place in playlist (alphabetically sorted)
}


// for (const levelFolder of await readdir(videoDir)) {t bestVideoPath = '';
//   for await (const videoFile of Deno.readDir(pbFolder)) {

//     if (existingPlaylistItems.find((item) => item.title === title)) {
//       console.log(`Video "${title}" already exists, skipping upload`);
//       continue;
//     }

//     console.log(`Uploading: ${title}`);
//     const { data: uploadedVideo } = await youtube.videos.insert({
//       part: ['snippet', 'status'],
//       requestBody: {
//         snippet: {
//           title: title,
//           description: description,
//         },
//         status: {
//           privacyStatus: 'unlisted',
//         },
//       },
//       media: {
//         body: createReadStream(bestVideoPath),
//       },
//     });
//     console.log(`Uploaded video with ID: ${uploadedVideo.id}`);

//     let position = existingPlaylistItems.length;
//     for (let i = levelIndex; i < LEVELS.length; i++) {
//       const index = existingPlaylistItems.findIndex((x) => x.levelName === LEVELS[i]);
//       if (index !== -1) {
//         position = index;
//         if (i === levelIndex) {
//           console.log('Removing old video from playlist...');
//           await youtube.playlistItems.delete({
//             id: existingPlaylistItems[position].id,
//           });
//           existingPlaylistItems.splice(position, 1);
//         }
//         break;
//       }
//     }

//     console.log('Adding video to playlist at position:', position);
//     const { data: addedPlaylistItem } = await youtube.playlistItems.insert({
//       part: ['snippet'],
//       requestBody: {
//         snippet: {
//           playlistId: playlist.id,
//           position,
//           resourceId: {
//             kind: 'youtube#video',
//             videoId: uploadedVideo.id,
//           },
//         },
//       },
//     });

//     existingPlaylistItems.splice(position, 0, {
//       id: addedPlaylistItem.id,
//       title,
//       levelName,
//     });
//   }
// }
