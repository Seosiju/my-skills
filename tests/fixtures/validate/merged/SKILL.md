---
first: &first {name: merged, description: Passing YAML merge}
second: &second {name: wrong, description: false}
<<: [*first, *second]
---
