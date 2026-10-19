# memory zonkler

like cheat engine but worse :)

#### benchmarks

| implementation   | first scan time                       | third scan time |
|------------------|---------------------------------------|-----------------|
| naive            | > 2 mins (bored of waiting to finish) | -               | 
| rayon first scan | 28 seconds                            | 28 seconds      |
| rayon all scans  | 28 seconds                            | 18 seconds      |
