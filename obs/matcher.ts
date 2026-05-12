
const Screens = ["Menu", "StartLevel", "EndLevel"] as const;
type Screen = typeof Screens[number];

export async function matchScreen(imageDataUrl: string): Promise<Screen | null> {
    return null;
}
