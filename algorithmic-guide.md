# LinkedIn Algorithm Reference (Q1 2026)

> Condensed from Trust Insights' "Unofficial LinkedIn Algorithm Guide, Q1 2026 Edition" (Penn et al., March 2026). Source guide is 138pp; this is the load-bearing content with verified architectural claims and actionable strategy. Padding, repeated framings, persona scenarios, and generic LinkedIn advice are removed.
>
> **Confidence levels** are marked throughout: `[H]` = high (directly sourced from LinkedIn engineering publications), `[M]` = medium (architectural inference or single-source), `[L]` = low (Trust Insights interpretation or unstated by LinkedIn).
>
> **Verification status:** Foundational papers (arXiv:2510.14223, arXiv:2602.12354) and the March 12, 2026 Hristo Danchev LinkedIn Engineering Blog post are real and say what this document claims. Trust Insights' synthesis is the most rigorous public take currently available.

---

## 1. The Architecture in One Page

LinkedIn replaced its prior "heterogeneous" feed system (5+ separate retrieval pipelines: trending, collaborative filtering, geo, EBR, etc.) with a unified two-stage AI pipeline. Publicly disclosed March 12, 2026. `[H]`

**Stage 1 — Retrieval (the gate):** Two parallel paths produce ~2,000 candidates from hundreds of millions of posts:

- **FishDB** (Rust-based engine, Li et al., Nov 2025): handles in-network content (connections, followed creators/companies). Hard 30-day content window. Cannot retrieve anything older through this path. P99 latency 40ms. 16 replicas × 48 partitions per replica.
- **Causal LLM** (Ramanujam et al., Oct 2025): handles out-of-network "suggested" content. Fine-tuned LLaMA-3 3B as a dual encoder. Single shared model encodes both members and items into a shared 3,072-dim embedding space. Mean-pooled across all tokens. Runs on 72 H100 GPUs (48 nearline + 24 retrieval). Sub-50ms retrieval from hundreds of millions of items via cosine similarity (GPU-RAR architecture: item embeddings live in GPU memory, no traditional DB lookup).

**Stage 2 — Ranking (Generative Recommender / "Feed SR"):** Sequential transformer (Hertel, Srivastava et al., Feb 2026; Danchev, March 2026). Processes each viewer's last 1,000+ interactions as a chronological causal sequence of interleaved (post, action) pairs. Uses Qwen3 0.6B for member profile embeddings, late-fused as context. Multi-gate Mixture-of-Experts (MMoE) with DCNv2 experts as the prediction head. Causal attention with position-weighted loss: most recent interaction = full weight, oldest ≈ 50% weight.

**Critical implication:** Both systems are **text-only**. Photos, video frames, and image content do not enter either AI system directly. Video transcripts do.

**Important rejection:** LinkedIn evaluated a text-prompt LLM ranker (architecturally equivalent to 360Brew) for feed ranking and **rejected it** — it did not outperform GR online. 360Brew is real and used on 8+ non-feed surfaces, but is NOT the feed ranker. Treat any guide claiming otherwise as wrong. `[H]`

---

## 2. Verified Numbers

| Metric | Value | Source |
|---|---|---|
| Causal LLM model | LLaMA-3 3B fine-tuned dual encoder | Ramanujam 2025 |
| Embedding dim (full) | 3,072 | Ramanujam 2025 |
| Embedding dim (Matryoshka, experimental) | 512 (Recall@10: 0.4225 vs 0.4242 full) | Ramanujam 2025 |
| Pooling method | Mean pooling, all tokens equal weight | Ramanujam 2025 |
| Training pairs | 5M member-item pairs from positive engagement | Ramanujam 2025 |
| Training hardware | 8× H100 per run, batch size 4/GPU | Ramanujam 2025 |
| Retrieval candidates | ~2,000 from hundreds of millions | Ramanujam 2025 |
| Retrieval latency | Sub-50ms (Causal LLM) / P99 40ms (FishDB) | Ramanujam 2025; Li 2025 |
| FishDB content window | Hard 30 days | Li 2025 |
| GR sequence length | 1,000+ interactions | Hertel 2026 |
| GR profile encoder | Qwen3 0.6B | Hertel 2026 |
| Member embedding refresh (retrieval) | ≤30 min after activity | Ramanujam 2025 |
| New post indexing SLA | ≤1 min | Ramanujam 2025 |
| New profile indexing SLA | ≤1 min | Ramanujam 2025 |
| GR profile embedding refresh | Daily | Hertel 2026 |
| GNN member embedding refresh | Daily | arXiv:2506.12700 |
| Causal LLM A/B test result | +0.8% revenue, +1.17% Daily Unique Professional Interactions for low-connection cohort | Ramanujam 2025 |
| Feed SR / GR A/B test result | +2.10% time spent | Hertel 2026 |
| Numerical features encoding | Percentile-bucketed, wrapped in special tokens | Danchev 2026 |
| Numerical encoding gain | 30× correlation between popularity signal and embedding similarity; +15% Recall@10 | Danchev 2026 |
| Training loss | InfoNCE with easy + hard negatives; hard negative sampling = +3.6% recall | Danchev 2026 |
| Negative engagement signal | Removed; positive-only history performs best | Ramanujam 2025 |
| MixLM (job search ranker) | 0.6B params, ~450× compression (900 tokens → 2 per item), 22K items/sec/GPU, +0.47% DAU | Li et al. 2025 (arXiv:2512.07846) |

---

## 3. The Causal LLM (Retrieval) — What the Embedding Actually Sees

The Causal LLM builds a "member prompt" from your profile text + your positive engagement history. It generates an item embedding for each post from the post text. Cosine similarity between member embedding and item embedding determines whether your post enters the candidate pool.

**What goes into the member prompt:** `[H]` headline, About, work history, education, skills, certifications, posts you've liked/commented/shared, posts where you had long dwell time. Photos do not. Recommendation text status is not publicly confirmed.

**Mean pooling implication:** every token contributes equally to the embedding. There is no positional weighting. A 200-word post with a sharp opening and a meandering middle produces a worse embedding than a 200-word post that stays on-topic throughout. `[H]`

**Positive-only history:** LinkedIn explicitly removed negative engagement events from the history sequence and found this significantly improves retrieval. Implication: things you scroll past or react negatively to do not pull your embedding around. Things you actually engage positively with do. `[H]`

**Cold start:** The LLM has world knowledge, so a new member's profile alone places them meaningfully in embedding space without engagement history. This is the +1.17% lift cohort. Day-one profile quality matters more than ever. `[H]`

**Member embedding refreshes within 30 minutes** of activity. New profile changes propagate within 1 minute. So profile edits and engagement actions take effect fast. `[H]`

---

## 4. The Generative Recommender (Ranking) — How It Actually Reads Behavior

GR is the production feed ranker. It is **not** an LLM that reads your post text. It is a sequential transformer that reads behavioral patterns. `[H]`

**Input:** A causal sequence of the viewer's last 1,000+ feed impressions, each as a (post embedding, action) pair, ordered oldest → newest.

**Profile context:** A dense embedding from Qwen3 0.6B encoding the viewer's profile text, late-fused as context after the transformer layers.

**Prediction:** Multi-task — predicts both passive (dwell, view) and active (like, comment, share, repost) engagement separately. MMoE with DCNv2 experts.

**Recency effect:** Position-weighted loss means the most recent interaction has the highest attention weight; the oldest position has ~50% weight. This creates a real recency bias in *what topics matter to a viewer right now*, but it operates on the viewer's behavior, not on post age. `[H]`

**Critical for creators:** GR does not read your comment text. It reads the fact that you commented on a post about topic X. Pattern is the signal. The post you commented ON contributes to your member embedding (in the retrieval system); your comment text does not. `[H]`

---

## 5. The Six Load-Bearing Insights

These are the only insights in the 138-page guide that meaningfully change how to operate. Everything else is consequence or restatement.

### 5.1 Topic coherence beats almost everything `[H]`
Mean pooling means a scattered profile/posting history averages toward the centroid of embedding space (low signal). A topically focused profile/posting history concentrates the vector in a meaningful region. This makes the retrieval system confident about who to surface you to and what audiences to retrieve for you. **Three to five tightly related content pillars > a broad founder voice across many topics.**

### 5.2 Profile is now a retrieval gate, not a discovery aid `[H]`
Vague headlines mathematically reduce reach because they degrade your member embedding's coherence. The retrieval gate runs *before* ranking. If your profile + history doesn't position you near a viewer's interests in embedding space, your content never enters their candidate pool — quality is irrelevant.

### 5.3 Recency as primary ranking logic is dead `[H]`
Within the 30-day FishDB window, relevance dominates. A 3-week-old post that matches a viewer's professional identity outranks a fresh post that doesn't. Posting time matters far less than commonly believed. The 30-day window is a hard architectural cutoff for in-network retrieval — content older than 30 days is not indexed in FishDB at all.

### 5.4 Comments >> reactions, and the mechanism is non-obvious `[H]`
A comment registers as a high-intent positive engagement. Two effects: (a) the *post you commented on* enters your retrieval prompt, pulling your member embedding toward that topic; (b) GR records the engagement pattern (you comment on topic X posts) and uses it for ranking. **Your own comment text doesn't matter to either system.** What matters is what you comment on.

### 5.5 Cold start is now actually solvable `[H]`
World knowledge in the LLM means a well-written profile places a brand-new account meaningfully in embedding space without behavioral data. First text committed to the system sets the initial coordinates. There is no prior architectural era where day-one positioning mattered this much.

### 5.6 Photos and visual content do not enter the AI systems `[H]`
Both retrieval and ranking are text-only. Photos affect outcomes only through the human engagement they generate, which then becomes signal. Video transcripts ARE processed; video frames are not. Image captions are the primary semantic input for image posts.

---

## 6. Operational Strategy — Profile

Profile text is the foundational input to both systems. Refresh propagates in ≤30 min for retrieval, daily for ranking.

**Headline (highest leverage field).** Lead with your 2-3 most important professional terms. State the value proposition. Use your audience's vocabulary, not yours. Up to 220 chars. Mean pooling means every word contributes equally — don't waste tokens on filler. `[H]`

**About section.** Largest narrative block both encoders see. Strong opening paragraph that summarizes core expertise. Weave skills into accomplishment narratives ("led the SEO strategy that resulted in...") rather than skill lists. Quantify achievements (numbers are semantically precise). Name notable companies/technologies (these link to Economic Graph nodes). `[H]`

**Experience.** Link each role to the official Company Page (clean Economic Graph link). Use precise titles and dates. Achievement-oriented descriptions (STAR method). Embed relevant skills naturally in each role. `[M]`

**Skills.** Pin top 3 most relevant. Use LinkedIn's standardized skill suggestions (maps to canonical Economic Graph nodes). Skill assessment badges add a verified credential. `[M]`

**Recommendations.** Whether they enter the AI encoding is **not publicly confirmed**. They do build human credibility, which drives engagement, which becomes signal. Recommender identity strengthens Economic Graph connection. `[L]`

**Photos.** Don't enter AI systems. Affect human trust → engagement → signal. Use real photos; LinkedIn detects AI-generated images and flags as negative trust signal. `[L]` for the AI-detection claim.

**Topical coherence across the whole profile.** This is the load-bearing concept. Headline, About, Experience, Skills, and posting topics should occupy the same semantic neighborhood. If profile says "B2B SaaS demand gen" but posts are about personal productivity, the embedding gets mixed signals and retrieval matching degrades. `[H]`

---

## 7. Operational Strategy — Content

### 7.1 Topic selection
Sit at the intersection of your established expertise + audience pain points + a perspective they can't easily find elsewhere. Specificity > breadth ("How mid-sized B2B SaaS companies use AI for competitive analysis" > "AI in marketing"). Topics should reflect your headline/About claims. `[H]`

### 7.2 Format selection
- **Text posts:** Full text is the primary retrieval input. Best for sharp insights and discussion-starters.
- **Articles/newsletters:** Rich semantic input; best for topical authority. Newsletter subscribers = recurring engagement signal.
- **Images/carousels:** Caption text is the embedding input — make it strong and topic-specific.
- **Native video:** Transcripts are processed. Add captions. Strong dwell-time generator.
- **Polls:** Quick broad engagement; useful for early visibility signals.
`[M]` (LinkedIn does not document format-specific weighting publicly)

### 7.3 Writing principles
- **Topical coherence throughout.** Mean pooling = every word contributes equally. A great hook + meandering middle = worse embedding than uniformly on-topic prose. `[H]`
- **Use your audience's vocabulary.** The terminology in their profiles is what your content needs to match. `[H]`
- **Single-focus posts > scattered topics.** Two focused posts > one sprawling post. `[H]`
- **Strong opening.** For human dwell time and scroll-stop. Long dwell is a Professional Interaction signal that trains the retrieval model. `[H]`
- **End with an open-ended question.** Drives comment threads. Reply to comments — that exchange becomes part of the commenter's engagement history, strengthening their connection to your topic. `[M]`
- **Avoid engagement bait.** "Comment YES if you agree"-style content is now actively suppressed (per LinkedIn VP Engineering Tim Jurka). `[H]`

### 7.4 Hashtags
3-5 relevant tags. Mix one or two broad with two or three niche. **Note:** LinkedIn has not publicly documented hashtag-specific feature extraction. Trust Insights infers contribution from architectural logic. `[L]`

### 7.5 Tagging people/companies
Tag only when genuinely relevant. Unfounded tags create noise.

### 7.6 Posting cadence
**Topic coherence > frequency.** Lower volume of consistent content > higher volume of scattered content. The guide does not specify an optimal cadence; the architectural implication is that you should post as often as you can without diluting topic focus. `[H]` for the principle, `[L]` for any specific cadence.

---

## 8. Operational Strategy — Engagement

Every engagement enters two systems simultaneously. Retrieval embedding refreshes ≤30 min; GR ranking processes the chronological sequence with recency-weighted attention.

### 8.1 Reactions
Each reaction logs in your member prompt (retrieval) and adds a position to your GR sequence. Recent reactions weight more in GR than older ones. **Off-topic reactions inject noise into both your embedding and your GR sequence.** Use varied reactions ("Insightful", "Celebrate") for richer signal. `[H]`

### 8.2 Comments (highest-leverage daily action)
- High-weight positive engagement event in retrieval.
- The text of the **post you comment on** enters your member prompt — so you're literally pulling your embedding toward that post's topic.
- Your comment text is read by neither system. The behavioral pattern is the signal.
- Add value, don't just agree. Triggers reply chains, which extend engagement and create more signal.

### 8.3 Groups
Concentrated stream of topically-aligned engagement. Both systems read this as evidence of sustained domain expertise (vs. passing curiosity). `[M]`

### 8.4 Connections
Each accepted connection adds a high-relevance content source to your feed. This shapes your future engagement opportunities (and therefore your embedding and GR sequence). Personalized notes drastically increase acceptance. Connect with people who already engage with your content — they'll keep doing so. `[M]`

### 8.5 LinkedIn Articles / Newsletters
Permanent assets that generate sustained engagement. Subscribers = recurring signal. `[M]`

### 8.6 Time-of-engagement
GR's recency weighting means recent engagement signals current topical interests more strongly than old engagement. Periodic re-engagement with your core topics resets and reinforces the recency-weighted signal. `[H]`

---

## 9. Cold-Start Playbook (New Account or New Topic Pivot)

`[H]` for the architecture; `[M]` for the operational sequencing.

1. **Profile first.** Day-one profile quality determines initial semantic coordinates. Headline, About, Experience, Skills must occupy the same tight semantic neighborhood as the audience you want to reach.
2. **Engagement before posting.** A few days of topical reactions and comments seeds your member embedding before you publish anything. Your first post then ships into a coherent embedding context.
3. **Target the top of your topic cluster.** Engage with established voices in your specific niche. The Causal LLM reads the *posts you engage with*, so engaging with on-topic content from credible voices is one of the fastest ways to position yourself.
4. **Post pillar content within the first two weeks.** Cold-start cohorts get the largest lift from the LLM-based retrieval (+1.17% DUPI). Capitalize while the system is most actively building your representation.
5. **Maintain topic discipline for ~30 days.** This is the duration of the FishDB window and roughly the period over which engagement patterns stabilize the embedding into a stable position.

---

## 10. What's Confirmed Production vs. What Isn't

| System | Status | Source |
|---|---|---|
| Causal LLM retrieval (LLaMA-3 3B dual encoder) | Confirmed production | arXiv:2510.14223; Danchev March 2026 |
| FishDB | Confirmed production (replaced FollowFeed) | Li et al. Nov 2025 |
| Generative Recommender / Feed SR | Confirmed production primary feed ranker | arXiv:2602.12354; Danchev March 2026 |
| SGLang for AI Job Search and AI People Search | Confirmed | Ramachandran et al. Feb 2026 |
| vLLM for 50+ GenAI applications | Confirmed | Zhai et al. Aug 2025 |
| MixLM job search ranker | Confirmed full-traffic Nov 2025 | arXiv:2512.07846 |
| GNN cross-domain candidate generation | Production (notification system primary; embeddings reusable across surfaces) | arXiv:2506.12700 |
| 360Brew | "Pre-production" per its own paper. Used on 8+ non-feed surfaces. **NOT** the feed ranker — LinkedIn evaluated and rejected text-prompt LLM rankers for feed. Paper recalled from arXiv late 2025 due to licensing. | arXiv:2501.16450v4 (recalled) |
| User-prompt-driven feed recommendation | Prototyping only | arXiv:2510.14223 |

---

## 11. Trust Insights' Bias Finding — Important Caveats

`[L]` for production relevance.

Trust Insights (Penn & Robbert, 2025) tested 406 paired posts with only the author's name changed against the **base** LLaMA-3 model. Found systematic embedding-position differences (Cohen's d = -0.93, p < 0.0001).

**Critical caveats:**
- Tested base LLaMA-3, not LinkedIn's production system.
- LinkedIn's Causal LLM went through three additional post-training stages: Continuous Pre-Training on trillions of LinkedIn-specific tokens, Instruction Fine-Tuning on proprietary datasets, Supervised Fine-Tuning on millions of labeled engagement examples.
- It is unknown whether this fine-tuning preserves, amplifies, or mitigates the bias.
- Treat as "potential concern to monitor" — not as direct measurement of LinkedIn's production behavior.

The architectural point that *does* hold: small cosine similarity differences are determinative at scale because retrieval returns top-K nearest neighbors. `[H]`

---

## 12. Things the Guide Hedges or Doesn't Cover

- **Promotional/paid content interleaving.** Final ranking stage and how organic competes with paid is not covered substantively.
- **Magnitude of reach declines.** Independent reports cite 50-65% post-reach decline since late 2025. Tim Jurka has confirmed this is partly by design (relevance over distribution). The guide does not surface this.
- **Hashtag mechanics.** No public LinkedIn engineering disclosure. All hashtag advice is inference.
- **Recommendation text in embeddings.** Not publicly confirmed.
- **Format-specific weighting.** Not publicly disclosed.
- **Exact retention window for out-of-network embedding-based retrieval index.** Only the 30-day FishDB window for connection-based content is documented.
- **Specific engagement multipliers** (e.g., "comments are 8-15x weight of reactions"): These exist in the broader ecosystem but are NOT in the Trust Insights guide and are not in any LinkedIn engineering publication. Treat as fabricated.

---

## 13. Quick-Reference Decision Rules

For an agent generating LinkedIn strategy:

- **"Should I post X?"** → Does X sit within my established 3-5 topic pillars? If no, either don't post or accept that this post pulls embedding coherence in a different direction.
- **"How long should the post be?"** → Long enough to maintain topical clarity throughout. Mean pooling means short on-topic > long with drift. No specific length guidance from LinkedIn engineering.
- **"When should I post?"** → Largely doesn't matter within the 30-day window. Recency is no longer the primary ranking signal. Optimize for when *humans* are likely to engage (drives early dwell signal), not for algorithmic time-of-day myths.
- **"Should I engage with this post?"** → Is it within my topic pillars? If yes, engage (it pulls your embedding toward your target neighborhood). If no, skip (off-topic engagement injects noise).
- **"Should I comment or react?"** → Comment > reaction always for high-intent content. Comment text doesn't matter to AI; the act of commenting and what you comment on does.
- **"Should I post this off-topic personal update?"** → Each off-topic post adds noise. If it's high human value (will earn strong engagement), the engagement signal partially offsets. Otherwise, skip or post in another channel.
- **"My profile is changing focus — when does it propagate?"** → Profile edits: ≤1 min for retrieval indexing, ≤30 min for member embedding refresh, daily for GR profile embedding refresh. Engagement effects: ≤30 min for retrieval embedding.
- **"Why isn't my old post getting traction anymore?"** → If it's >30 days old, it's no longer in the FishDB window for connection-based retrieval. Out-of-network retention is undocumented but likely also bounded.
- **"Is this advice from elsewhere reliable?"** → If it cites specific engagement multipliers, optimal posting times, or "the algorithm rewards X format," it's likely not from LinkedIn engineering publications. Verify against arXiv:2510.14223, arXiv:2602.12354, or the March 12, 2026 Danchev blog post.

---

## 14. Primary Sources

**Foundational papers (confirmed real, accessible):**
- Ramanujam, S. S., Alonso, A., Kataria, S., et al. (Oct 16, 2025). *Large Scale Retrieval for the LinkedIn Feed using Causal Language Models.* arXiv:2510.14223
- Hertel, L., Srivastava, G., Naqvi, S. A., et al. (Feb 12, 2026). *An Industrial-Scale Sequential Recommender for LinkedIn Feed Ranking.* arXiv:2602.12354
- Li, G., He, R., Jing, S., et al. (Nov 25, 2025). *MixLM: High-throughput and effective LLM ranking via text-embedding mix-interaction.* arXiv:2512.07846

**LinkedIn Engineering Blog posts:**
- Danchev, H. (March 12, 2026). *Engineering the next generation of LinkedIn's Feed.*
- Li, K., Raveendran, N., Gupta, P., et al. (Nov 17, 2025). *FishDB: A generic retrieval engine for scaling LinkedIn's feed.*
- Ramachandran, S. R., et al. (Feb 20, 2026). *Scaling LLM-based ranking systems with SGLang at LinkedIn.*
- Shimizu, S., et al. (Dec 9, 2025). *Turbocharging LinkedIn's recommendation systems with SGLang.*
- Zhai, Y., et al. (Aug 26, 2025). *How we leveraged vLLM to power our GenAI applications at LinkedIn.*

**Recalled but architecturally informative:**
- Zhang, H., et al. (2025). *360Brew: A decoder-only foundation model for personalized ranking and recommendation.* arXiv:2501.16450v4 (recalled late 2025; describes real system but not the feed ranker)

---

## 15. Source Document Provenance

This reference is condensed from: Trust Insights, *The Unofficial LinkedIn Algorithm Guide, Q1 2026 Edition*, by Christopher Penn et al.
