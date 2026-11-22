# Yaak workspace

This folder is a [Yaak](https://yaak.app/) workspace for the public API of Strata, in the Yaak directory-sync format: one `yaak.<id>.yaml` file for each workspace, environment, folder and request. It has one request for each route of `strata::api::router`.

## Open

In Yaak, select **Open folder** and choose `dev/yaak`. Yaak keeps the workspace and this folder in sync: a change in Yaak changes the files, and a `git pull` changes the workspace.

## Run the API

Do steps 1 to 4 of "On a laptop" in [tech design 0049](../../docs/tech-design/0049-walking-skeleton.md). Then start the API with the development values of `compose.yaml`:

```sh
STRATA_DATABASE_URL=postgres://strata_api:strata_api@127.0.0.1:6432/strata \
AWS_ENDPOINT=http://127.0.0.1:3900 AWS_ALLOW_HTTP=true \
AWS_ACCESS_KEY_ID=GK000000000000000000000000 \
AWS_SECRET_ACCESS_KEY=0000000000000000000000000000000000000000000000000000000000000000 \
AWS_DEFAULT_REGION=garage AWS_BUCKET_NAME=strata \
cargo run -p strata -- api
```

## Get a token

```sh
cargo run -p strata-dev -- token --tenant <tenant value> --sub alice
```

Use the tenant value of a binding from step 4. A token is valid for one hour.

In Yaak, make a sub-environment, keep it private, and set `token` in it. Yaak does not write a private environment to this folder. Do not set the token in "Global Variables": that environment is a file in git.

## Send

Send the requests of **New document** in order: start, upload the file, finish. Before the upload, choose the file in the Body tab, and set `size` of the start to the size of that file. Each later request reads the IDs and the signed URL from the last response of its start.

**New revision** starts the next revision of the document of **New document**.

## Change the workspace

When you add a route to `strata::api::router`, add a request for it in Yaak and commit the new file. Yaak writes these files, so yamllint does not check this folder.
