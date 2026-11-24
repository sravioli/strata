# Yaak workspace

This folder is a [Yaak](https://yaak.app/) workspace for the public API of Strata, in the Yaak directory-sync format: one `yaak.<id>.yaml` file for each workspace, environment, folder and request. It has one request for each route of `strata::api::router`.

## Open

In Yaak, select **Open folder** and choose `dev/yaak`. Yaak keeps the workspace and this folder in sync: a change in Yaak changes the files, and a `git pull` changes the workspace.

## Run the API

```sh
make dev
```

It starts the services, the API on `127.0.0.1:8080` and the worker, and binds the tenant values `acme` and `globex`. `base_url` in "Global Variables" is `http://127.0.0.1:8080`. Keep it as plain text: a value in Yaak's encrypted form opens only on the machine that encrypted it.

## Get a token

```sh
target/debug/strata-dev token --tenant acme --sub alice
```

A token is valid for one hour. Use `globex` for a token of the second tenant.

In Yaak, make a sub-environment, keep it private, and set `token` in it. Yaak does not write a private environment to this folder. Do not set the token in "Global Variables": that environment is a file in git.

## Send

Send the requests of **New document** in order: start, upload the file, finish. Before the upload, choose the file in the Body tab, and set `size` of the start to the size of that file. Each later request reads the IDs and the signed URL from the last response of its start.

**New revision** starts the next revision of the document of **New document**.

**Status** reads the status of the revision of its folder. Send it again after the time of `Retry-After`, until the state is final. **Search and status** has the two `QUERY` routes: the status of the revisions of both folders, and a search. Put a word of your file in `q`.

## Change the workspace

When you add a route to `strata::api::router`, add a request for it in Yaak and commit the new file. Yaak writes these files, so yamllint does not check this folder.
