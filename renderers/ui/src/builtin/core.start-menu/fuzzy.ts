// Small fuzzy matcher: every query character must appear in order. Matches at word starts and
// consecutive matches score higher; a plain substring match gets a large bonus.

export function fuzzyScore(query: string, text: string): number {
	const q = query.trim().toLowerCase();
	if (!q) return 1;
	const t = text.toLowerCase();
	const sub = t.indexOf(q);
	let score = 0;
	let ti = 0;
	let streak = 0;
	for (const ch of q) {
		if (ch === ' ') continue;
		const found = t.indexOf(ch, ti);
		if (found < 0) return 0;
		const wordStart = found === 0 || /[\s\-_.]/.test(t[found - 1]);
		streak = found === ti ? streak + 1 : 0;
		score += 1 + (wordStart ? 6 : 0) + streak * 3 - Math.min(found - ti, 6) * 0.3;
		ti = found + 1;
	}
	if (sub >= 0) score += 20 + (sub === 0 ? 15 : 0);
	return score + 1 / (1 + t.length);
}

export function fuzzyFilter<T>(items: T[], query: string, key: (item: T) => string): T[] {
	return items
		.map((item) => ({ item, s: fuzzyScore(query, key(item)) }))
		.filter((x) => x.s > 0)
		.sort((a, b) => b.s - a.s)
		.map((x) => x.item);
}
