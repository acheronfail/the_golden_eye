/**
 * Much inspiration taken from: https://github.com/jakzo/NeonWhiteMods/blob/main/scripts/upload-to-youtube.ts
 */

import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { readdir, readFile, stat, writeFile } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { google } from 'googleapis';
import chalk from 'chalk';
import open from 'open';
import { checkbox, select } from '@inquirer/prompts';
import { createYoutubeTitle, parseVideoName, parseYoutubeTitle } from '../obs/naming.ts';

//
// Setup
//

const { GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET } = process.env;
if (!GOOGLE_CLIENT_ID || !GOOGLE_CLIENT_SECRET) {
  console.error('Missing GOOGLE_CLIENT_ID or GOOGLE_CLIENT_SECRET in environment');
  process.exit(1);
}

const __dirname = dirname(fileURLToPath(import.meta.url));

const [, , videoDir] = process.argv;
if (!videoDir || process.argv.length !== 3) {
  console.error('Usage: just upload <pbPlaylistTitle> <allPlaylistTitle> <videoDir>');
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
// Prompt for which playlists to upload to
//

interface League {
  name: string;
  extraTag?: string;
}

const leagues: Record<string, League> = {
  standard: {
    name: 'Goldeneye Speedruns',
  },
  enemyRockets: {
    name: 'Goldeneye Speedruns - Enemy Rockets',
    extraTag: 'Enemy Rockets',
  },
};

const chosenLeague =
  leagues[
    await select({
      message: 'Select PB playlist to upload to',
      choices: Object.keys(leagues),
    })
  ];

const allPlaylistTitle = 'Goldeneye';
const pbPlaylistTitle = chosenLeague.name;

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

const oauth2Client = new google.auth.OAuth2(GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET, `http://localhost:${port}`);

if (await stat(tokenPath).catch(() => false)) {
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
}

server.close();
server.closeAllConnections();
await new Promise((resolve) => server.on('close', resolve));

//
// Read Playlists
//

const youtube = google.youtube({ version: 'v3', auth: oauth2Client });

console.log(chalk.grey('- Finding playlists...'));
youtube;
const { data: existingPlaylists } = await youtube.playlists.list({
  part: ['snippet'],
  mine: true,
});

let allPlaylist = existingPlaylists.items?.find((item) => item?.snippet?.title === allPlaylistTitle);
if (allPlaylist) {
  console.log(chalk.grey(`- Found All playlist "${chalk.cyan(allPlaylistTitle)}" (ID: ${allPlaylist.id})`));
} else {
  console.log(chalk.grey(`- All playlist "${chalk.red(allPlaylistTitle)}" not found, creating...`));
  ({ data: allPlaylist } = await youtube.playlists.insert({
    part: ['snippet', 'status'],
    requestBody: {
      snippet: {
        title: allPlaylistTitle,
        description: 'All of my Goldeneye N64 speedrun videos',
      },
      status: {
        privacyStatus: 'unlisted',
      },
    },
  }));
  console.log(chalk.grey(`- Created "${chalk.green(allPlaylistTitle)}" (ID: ${allPlaylist.id})`));
}

let pbPlaylist = existingPlaylists.items?.find((item) => item?.snippet?.title === pbPlaylistTitle);
if (pbPlaylist) {
  console.log(chalk.grey(`- Found PB Playlist "${chalk.cyan(pbPlaylistTitle)}" (ID: ${pbPlaylist.id})`));
} else {
  console.log(chalk.grey(`- PB Playlist "${chalk.red(pbPlaylistTitle)}" not found, creating...`));
  ({ data: pbPlaylist } = await youtube.playlists.insert({
    part: ['snippet', 'status'],
    requestBody: {
      snippet: {
        title: pbPlaylistTitle,
        description: 'PBs for Goldeneye N64 speedruns',
      },
      status: {
        privacyStatus: 'unlisted',
      },
    },
  }));
  console.log(chalk.grey(`- Created "${chalk.green(allPlaylistTitle)}" (ID: ${allPlaylist.id})`));
  await new Promise((resolve) => setTimeout(resolve, 5_000));
}

const existingPlaylistItems: {
  id: string;
  title: string;
  levelName: string | undefined;
}[] = [];

console.log(chalk.grey(`- Listing all videos in "${chalk.cyan(pbPlaylistTitle)}"...`));
let nextPageToken: string | undefined = undefined;
do {
  const { data: playlistItems } = await youtube.playlistItems.list({
    playlistId: pbPlaylist.id!,
    part: ['snippet'],
    pageToken: nextPageToken,
  });

  for (const item of playlistItems.items ?? []) {
    existingPlaylistItems.push({
      id: item.id!,
      title: item.snippet!.title!,
      levelName: item.snippet!.title!.match(/\] (.+?) - /)?.[1],
    });
  }

  nextPageToken = playlistItems.nextPageToken as string;
} while (nextPageToken);

console.log(chalk.grey(`- Found ${existingPlaylistItems.length} videos in "${chalk.cyan(pbPlaylistTitle)}"`));

//
// Upload Videos
//

for (const videoPath of videosToUpload) {
  const videoFileName = basename(videoPath, '.mp4');
  const currentNameParts = parseVideoName(videoFileName);
  if (!currentNameParts) {
    console.warn(chalk.yellow(`- Skipping video with unrecognized name format: ${videoFileName}`));
    continue;
  }

  const { title, description } = createYoutubeTitle(currentNameParts);

  if (existingPlaylistItems.find((item) => item.title === title)) {
    console.warn(chalk.yellow(`- Video "${title}" already exists in playlist, skipping upload`));
    continue;
  }

  console.log(chalk.grey(`- ${chalk.magenta('Uploading video')}: ${chalk.cyan(title)}...`));
  const { data: uploadedVideo } = await youtube.videos.insert({
    part: ['snippet', 'status'],
    requestBody: {
      snippet: {
        title,
        description,
      },
      status: {
        privacyStatus: 'unlisted',
      },
    },
    media: {
      body: createReadStream(videoPath),
    },
  });
  const link = `https://youtu.be/${uploadedVideo.id}`;
  console.log(chalk.grey(`- Uploaded "${chalk.green(title)}": ${chalk.white(link)}`));

  let videoPosition = existingPlaylistItems.findIndex((item) => {
    const parts = parseYoutubeTitle(item.title);
    if (!parts) return false;

    const { levelNumber, difficultyNumber } = parts;

    if (levelNumber > currentNameParts.levelNumber) return true;
    if (difficultyNumber > currentNameParts.difficultyNumber) return true;

    return levelNumber === currentNameParts.levelNumber && difficultyNumber === currentNameParts.difficultyNumber;
  });

  if (videoPosition === -1) {
    videoPosition = existingPlaylistItems.length;
  }

  const videoAtPosition = existingPlaylistItems[videoPosition];
  const parts = videoAtPosition && parseYoutubeTitle(videoAtPosition.title);
  const isSameLevel =
    parts &&
    parts.levelNumber === currentNameParts.levelNumber &&
    parts.difficultyNumber === currentNameParts.difficultyNumber;
  const isBetterTime = parts && currentNameParts.time < parts.time;

  // remove video currently at that position if it has a worse time than the one we're adding
  if (isSameLevel && isBetterTime) {
    console.log(
      chalk.grey(`
      - Video "${chalk.red(videoAtPosition.title)}" has worse time, removing from playlist...`),
    );
    await youtube.playlistItems.delete({ id: videoAtPosition.id });
    existingPlaylistItems.splice(videoPosition, 1);
  }

  // add the video to the pb playlist
  if (!videoAtPosition || !isSameLevel || isBetterTime) {
    console.log(chalk.grey(`- Adding video to "${chalk.cyan(pbPlaylistTitle)}" playlist at ${videoPosition}...`));
    const { data: addedPlaylistItem } = await youtube.playlistItems.insert({
      part: ['snippet'],
      requestBody: {
        snippet: {
          playlistId: pbPlaylist.id,
          position: videoPosition,
          resourceId: {
            kind: 'youtube#video',
            videoId: uploadedVideo.id,
          },
        },
      },
    });

    existingPlaylistItems.splice(videoPosition, 0, {
      id: addedPlaylistItem.id!,
      title,
      levelName: currentNameParts.level,
    });
  }

  // always add the video to the all videos playlist, even if it's not a PB
  console.log(chalk.grey(`- Adding video to "${chalk.cyan(allPlaylistTitle)}" playlist...`));
  await youtube.playlistItems.insert({
    part: ['snippet'],
    requestBody: {
      snippet: {
        playlistId: allPlaylist.id,
        resourceId: {
          kind: 'youtube#video',
          videoId: uploadedVideo.id,
        },
      },
    },
  });
}
