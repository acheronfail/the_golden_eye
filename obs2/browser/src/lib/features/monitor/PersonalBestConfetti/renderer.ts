interface ConfettiPiece {
	x: number;
	lift: number;
	spread: number;
	delay: number;
	size: number;
	rotation: number;
	spin: number;
	color: string;
}

const COLORS = ['#ffd761', '#fff0b0', '#ffffff', '#88e8b1', '#9cbcff', '#e5a2ec'];

export const makeConfetti = (seed: number): ConfettiPiece[] => {
	const random = (index: number) => {
		const n = Math.sin(index * 127.1 + seed * 311.7) * 43758.5453;
		return n - Math.floor(n);
	};
	return Array.from({ length: 180 }, (_, index) => ({
		x: index % 2 === 0 ? 0.08 : 0.92,
		lift: 2.1 + random(index * 7) * 1.2,
		spread: 0.35 + random(index * 7 + 1) * 0.8,
		delay: random(index * 7 + 2) * 0.08,
		size: 4 + random(index * 7 + 3) * 5,
		rotation: random(index * 7 + 4) * Math.PI,
		spin: (random(index * 7 + 5) - 0.5) * 24,
		color: COLORS[Math.floor(random(index * 7 + 6) * COLORS.length)]
	}));
};

export const drawConfettiFrame = (
	ctx: CanvasRenderingContext2D,
	width: number,
	height: number,
	pieces: ConfettiPiece[],
	progress: number,
	reducedMotion: boolean
): boolean => {
	ctx.clearRect(0, 0, width, height);
	if (progress >= 1) return false;
	ctx.globalAlpha = Math.min(1, (1 - progress) / 0.2);
	for (const [index, piece] of pieces.entries()) {
		const t = progress - piece.delay;
		if (!reducedMotion && t < 0) continue;
		// Compress the entire ascent, then keep the original gravity for the fall.
		const ascentSpeed = 2.8;
		const apexTime = piece.lift / 7.4;
		const ascentDuration = apexTime / ascentSpeed;
		const flightTime = t < ascentDuration ? t * ascentSpeed : apexTime + t - ascentDuration;
		const direction = piece.x < 0.5 ? 1 : -1;
		const x = reducedMotion
			? ((index % 18) + 0.5) / 18
			: piece.x + direction * piece.spread * (1 - Math.exp(-3 * ascentSpeed * t));
		const y = reducedMotion
			? 0.08 + Math.floor(index / 18) * 0.045
			: 0.85 - piece.lift * flightTime + 3.7 * flightTime * flightTime;
		ctx.save();
		ctx.translate(x * width, y * height);
		ctx.rotate(piece.rotation + (reducedMotion ? 0 : piece.spin * t));
		ctx.fillStyle = piece.color;
		const flutter = reducedMotion ? 1 : Math.max(0.2, Math.abs(Math.cos(piece.spin * t)));
		ctx.fillRect(-piece.size / 2, -piece.size / 4, piece.size * flutter, piece.size / 2);
		ctx.restore();
	}
	ctx.globalAlpha = 1;
	return true;
};
