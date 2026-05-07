# Real Repository Baselines

These baselines capture cold structural indexing performance and language
coverage for large public repositories.

Regenerate a baseline with:

```sh
scripts/bench-real-repo.sh django https://github.com/django/django.git main
scripts/bench-real-repo.sh nextjs https://github.com/vercel/next.js.git canary
```

Use `ACI_BENCH_VARIANT=scanner-only` or another benchmark variant to compare
extraction modes.
