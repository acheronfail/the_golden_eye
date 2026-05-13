import blessed, { type Widgets } from 'blessed';
import figlet from 'figlet';
import type { LevelInfo } from './parse.ts';

type StatusBoxOptions = {
  borderColor: string;
  borderThick?: boolean;
  rainbowBorder?: boolean;
  rainbowHeading?: boolean;
  headingColor: string;
  headingText: string;
  headingFont?: string;
  hintText: string;
};

const RAINBOW = ['red', '#ff8800', 'yellow', 'green', 'cyan', 'blue', 'magenta'];

function colourize(text: string, offset = 0): string {
  return [...text].map((ch, i) => `{${RAINBOW[(i + offset) % RAINBOW.length]}-fg}${ch}{/}`).join('');
}

function colourizeNonWhitespace(text: string, offset = 0): string {
  let colourIndex = 0;
  return [...text]
    .map((ch) => {
      if (ch === '\n' || ch === ' ') {
        return ch;
      }

      const tagged = `{${RAINBOW[(colourIndex + offset) % RAINBOW.length]}-fg}${ch}{/}`;
      colourIndex += 1;
      return tagged;
    })
    .join('');
}

function addRainbowBorder(box: Widgets.BoxElement, screen: Widgets.Screen): void {
  const topOuter = blessed.text({
    parent: box,
    top: 0,
    left: 0,
    height: 1,
    tags: true,
  });

  const leftOuter = blessed.text({
    parent: box,
    top: 0,
    left: 0,
    width: 1,
    tags: true,
  });

  const rightOuter = blessed.text({
    parent: box,
    top: 0,
    right: 0,
    width: 1,
    tags: true,
  });

  const bottomOuter = blessed.text({
    parent: box,
    bottom: 0,
    left: 0,
    height: 1,
    tags: true,
  });

  const render = () => {
    const width = Math.max(2, screen.cols);
    const height = Math.max(2, screen.rows);

    const horizontal = '█'.repeat(width);
    topOuter.setContent(colourize(horizontal, 0));
    bottomOuter.setContent(colourize(horizontal, 4));

    const vertical = Array.from({ length: height }, (_, i) => colourize('█', i)).join('\n');
    leftOuter.setContent(vertical);
    rightOuter.setContent(Array.from({ length: height }, (_, i) => colourize('█', i + 3)).join('\n'));
  };

  render();
  screen.on('resize', render);
}

function createStatusBox(screen: Widgets.Screen, options: StatusBoxOptions): Widgets.BoxElement {
  const box = blessed.box({
    top: 'center',
    left: 'center',
    width: '100%',
    height: '100%',
    tags: true,
    border: options.rainbowBorder
      ? undefined
      : {
          type: 'line',
        },
    style: {
      fg: 'white',
      border: {
        fg: options.borderColor,
        bg: options.borderThick ? options.borderColor : undefined,
      },
    },
  });

  screen.append(box);

  if (options.rainbowBorder) {
    addRainbowBorder(box, screen);
  }

  const figletText = figlet.textSync(options.headingText, { font: options.headingFont || 'Standard' });
  const heading = options.rainbowHeading
    ? colourizeNonWhitespace(figletText)
    : `{${options.headingColor}-fg}${figletText}{/}`;
  const figletWidth = Math.max(...figletText.split('\n').map((l) => l.length), options.hintText.length + 1);
  const hintPadding = Math.max(0, Math.floor((figletWidth - options.hintText.length) / 2));
  const centredHint = ' '.repeat(hintPadding) + options.hintText;
  const content = `${heading}\n{gray-fg}${centredHint}{/}`;

  blessed.text({
    parent: box,
    top: 'center',
    left: 'center',
    width: figletWidth,
    height: 'shrink',
    tags: true,
    content,
  });

  return box;
}

export const createWelcomeBox = (screen: Widgets.Screen) =>
  createStatusBox(screen, {
    borderColor: '#FFD700',
    borderThick: false,
    headingColor: '#FFD700',
    headingText: 'The Golden Eye',
    headingFont: 'Tmplr',
    hintText: 'press space to monitor',
  });

export const createWaitingBox = (screen: Widgets.Screen) =>
  createStatusBox(screen, {
    borderColor: 'white',
    borderThick: false,
    headingColor: 'white',
    headingText: 'Waiting...',
    headingFont: 'ANSI Regular',
    hintText: 'waiting for a level to start...',
  });

export const createRecordingBox = (screen: Widgets.Screen) =>
  createStatusBox(screen, {
    borderThick: true,
    borderColor: 'green',
    headingColor: 'green',
    headingText: 'Recording',
    headingFont: 'Terrace',
    hintText: 'waiting for level finish...',
  });

export const createLevelStartBox = (screen: Widgets.Screen) =>
  createStatusBox(screen, {
    borderColor: 'white',
    headingColor: 'white',
    headingText: 'Level Start',
    headingFont: 'ANSI Regular',
    hintText: 'recording started, good luck!',
  });

export const createLevelFailedBox = (screen: Widgets.Screen) =>
  createStatusBox(screen, {
    borderThick: true,
    borderColor: 'red',
    headingColor: 'red',
    headingText: 'Failed',
    headingFont: 'Bloody',
    hintText: 'better luck next time!',
  });

export const createLevelCompleteBox = (screen: Widgets.Screen) =>
  createStatusBox(screen, {
    borderColor: 'white',
    headingColor: 'white',
    headingText: 'Complete',
    headingFont: 'ANSI Regular',
    hintText: 'drum roll please...',
  });

export const createStatisticsBox = (screen: Widgets.Screen) =>
  createStatusBox(screen, {
    borderThick: false,
    borderColor: 'white',
    headingColor: 'white',
    headingText: 'Reading Stats...',
    headingFont: 'Terrace',
    hintText: 'reading stats, feel free to keep going',
  });


export const createWarningBox = (screen: Widgets.Screen, message: string) =>
  createStatusBox(screen, {
    borderThick: false,
    borderColor: 'yellow',
    headingColor: 'yellow',
    headingText: 'Warning!',
    headingFont: 'Terrace',
    hintText: message,
  });

export const createLevelInfoBox = (screen: Widgets.Screen, levelInfo: LevelInfo) => {
  const minutes = Math.floor(levelInfo.time / 60);
  const seconds = levelInfo.time % 60;
  const timeString = `Time: ${minutes}:${seconds.toString().padStart(2, '0')}`;

  const hintText = `${levelInfo.difficulty} - ${levelInfo.level} - (PB: ${levelInfo.bestTime})`;

  return createStatusBox(screen, {
    borderColor: '#FFD700',
    headingColor: '#FFD700',
    headingFont: 'ANSI Regular',
    headingText: timeString,
    hintText,
  });
};

export const createNewPbBox = (screen: Widgets.Screen, time: number) => {
  const minutes = Math.floor(time / 60);
  const seconds = time % 60;
  const hintText = `New PB: ${minutes}:${seconds.toString().padStart(2, '0')}`;

  return createStatusBox(screen, {
    borderThick: true,
    rainbowBorder: true,
    rainbowHeading: true,
    borderColor: 'magenta',
    headingColor: 'magenta',
    headingText: 'New best time!',
    headingFont: 'Terrace',
    hintText,
  });
};
