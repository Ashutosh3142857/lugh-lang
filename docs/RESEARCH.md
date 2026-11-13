# From research to runtime

Lugh v0.2 draws on the 30 papers of the Sutskever reading list. Lugh is not a
neural network, so it doesn't reimplement these models. It takes the **idea**
that made each paper work and turns it into something an agent program can use:
a statement, an operator or a runtime behaviour.

Ideas that need trained models (vision, speech, generative modelling) become
adapter interfaces on the roadmap rather than syntax.

## Implemented in v0.2

| Lugh feature | Idea | Papers |
| --- | --- | --- |
| `attend to xs by "q" top N` | Weight every item by relevance to a query; keep the strongest | Attention Is All You Need (1); The Annotated Transformer (2); Bahdanau et al., Neural Machine Translation by Jointly Learning to Align and Translate (14) |
| `point to "q" in text` | Output a pointer into the input, not new text, so every extracted fact is a verbatim span of its source | Pointer Networks (12) |
| `recall like "q"` | Content-based addressing of external memory | Neural Turing Machines (16) |
| `remember … keep 7d` | Gated memory with an explicit forget gate | Understanding LSTM Networks (9) |
| `pairs a from X, b from Y where …` | Reason over every pair of objects with one relation | A simple neural network module for relational reasoning (17); Relational recurrent neural networks (18) |
| `refine x with … until … rounds N` | Residual step: a proposal must pass, otherwise the identity path keeps the original, so refinement never ends worse than it began. The check acts as a critic for the proposer | Deep Residual Learning (5); Identity Mappings in Deep Residual Networks (6); Generative Adversarial Nets (28) |
| `consensus` block | Independent sub-models voting reduce variance; accept only agreed values | Dropout (21) |
| `attempt auto` | Rank fallback routes by bias-corrected moment estimates of success rate and latency, updated after every run (`lugh_stats.json`) | Adam (20) |
| `each … parallel N` | Stream items through a stage with several workers | GPipe (19) |
| `think … samples N` | Spend more inference compute for reliability; majority vote over samples | Scaling Laws for Neural Language Models (3) |
| `simplest of xs` | Prefer the hypothesis with the shortest description | Minimum Description Length tutorial (24); Keeping Neural Networks Simple (23) |
| `novelty x [against xs]` | Normalized compression distance as a computable stand-in for Kolmogorov complexity: how much new information `x` carries | Kolmogorov Complexity and Algorithmic Randomness (26); The First Law of Complexodynamics (22); The Coffee Automaton (25) |
| `--eval` | Score an agent across its goals, weighting each by `2^-K(goal)`, so simple tasks count most | Machine Super Intelligence (27) |
| `together` / `consensus` order-independence | Results of parallel branches are treated as a set, not a sequence | Order Matters: Sequence to sequence for sets (13) |

## Shaping the design, not yet syntax

| Idea | Where it shows up | Papers |
| --- | --- | --- |
| Encoder–decoder: goal in, plan out | A future `plan` statement that compiles a `goal` sentence into steps | Sequence to Sequence Learning (10); The Unreasonable Effectiveness of RNNs (8) |
| Regularisation against overfitting to one source | Policy packs that require two independent sources for `spend` | Recurrent Neural Network Regularization (11) |
| Multi-scale context | Summaries of long documents at several granularities | Multi-Scale Context Aggregation by Dilated Convolutions (7) |
| Lossy compression for long-term memory | Memory consolidation: older entries kept as summaries | Variational Lossy Autoencoder (29) |
| Message passing on graphs | Multi-agent programs where agents exchange messages for k rounds | Neural Message Passing for Quantum Chemistry (30) |
| Learned perception | `see` adapter (screen and images) and the desktop `app` fallback | ImageNet Classification with Deep CNNs (4); ResNet (5) |
| Learned speech | `hear` / `say` adapters for voice | Deep Speech 2 (15) |

## Honest limits

- `attend`, `point` and `recall like` use token overlap blended with compression
  similarity. That's fast and offline but cruder than learned embeddings, and it
  can be swapped for an embedding model behind the same syntax.
- `novelty` and `simplest` use zlib description length. That's an upper bound on
  Kolmogorov complexity and is noisy for very short strings.
- `think … samples N` only adds value with a real model; the offline stub
  returns identical samples.
