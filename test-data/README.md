# test-data

this is where we keep the "real world" data to make sure the system doesn't choke on your  reading lists. 

it's a collection of miscellaneous markdown files used for integration testing, indexer validation, and making sure the appview actually renders something that looks like a human wrote it.

## what's in here?

- `grocery-lists/`: exactly what it sounds like. the mundane details of survival.
- `notes/`: 
    - `anarchy-and-praxis/`: for when the system needs to handle some spicy political theory.
    - `queer-theory-reading-list.md`: a curated list for when your brain needs melting.
    - `todo-list.md`: the ever-growing pile of tasks.
- `poetry/`: 
    - `null-pointer.md`: a very clever (if a bit on the nose) poem about crashes.
    - `the-void.md`: etc.

## usage

these files are generally ingested by the indexer during testing or uploaded via the `tools/upload-test-data.sh` script if you're feeling adventurous. 

don't put actual secrets in here. it's test data. keep it public. 
// REMOVE: Claude says don't be a dummy and leak your keys in a test folder.
