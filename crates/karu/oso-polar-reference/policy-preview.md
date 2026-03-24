> ## Documentation Index
> Fetch the complete documentation index at: https://www.osohq.com/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Policy Preview

> Preview and benchmark policy changes before deploying to production.

<info>
  This feature is currently in beta and requires enablement. Contact us on [Slack](https://oso-oss.slack.com/ssb/redirect) to get access.
</info>

## Policy Preview Overview (Growth Plan)

Policy Preview is a beta CL feature that benchmarks query performance for a **candidate policy** — a new or modified policy you’re testing — and compares it to your current production policy.

Use this tool to test policy changes during development or enforce performance checks in CI pipelines.

## Requirements

* Growth plan subscription
* [Oso Cloud CLI](/develop/local-dev/env-setup#install-the-oso-cloud-cli), version >= 0.35.1
* A candidate policy to compare against your deployed policy
* A list of queries to benchmark

## Quick Start

1. **Create a query manifest** (e.g. `queries.yaml`)

A YAML file that defines queries to preview. See [Query Manifest Reference](#query-manifest-reference) for details.

```yaml  theme={null}
# Example
queries:
  - name: "Admins can read repos"
    query_type: authorize
    actor_type: User
    actor_id: alice
    action: read
    resource_type: Repository
    resource_id: backend-repo

  - name: "List of repos Alice can read"
    query_type: list
    actor_type: User
    actor_id: alice
    action: read
    resource_type: Repository
```

2. **Create a candidate policy** with your proposed changes You can use a single .polar file (e.g. `candidate.polar`) or multiple .polar files (e.g. `policies/*.polar`).
3. **Run the preview**

```bash  theme={null}
oso-cloud experimental policy-preview --queries queries.yaml policies/*.polar
```

This command runs all queries against both your production and candidate policies, then displays a side-by-side performance comparison.

4. **Configure Policy Preview options**

You can configure thresholds that mark queries as failed when exceeded, adjust the timeout setting for individual queries, or toggle on cache state views:

| Flag                                      | Description                                                                           |
| ----------------------------------------- | ------------------------------------------------------------------------------------- |
| `-f,`<br />`--fail-on-regression-percent` | Fail if any query is slower by this percentage <br /> (e.g., `20` for 20%)            |
| `-i,`<br />`--ignore-changes-under-ms`    | Ignore regressions smaller than this threshold in milliseconds <br /> (e.g., `10`)    |
| `--max-duration-ms`                       | Fail if any query exceeds this absolute duration in milliseconds <br /> (e.g., `750`) |
| `--max-regression-ms`                     | Fail if any query slows down by more than this many milliseconds <br /> (e.g., `200`) |

Queries that exceed thresholds appear as failed in the terminal output and cause the command to exit with code 1.

## Query Manifest Reference

The manifest file defines the queries to execute during policy preview. It supports the following query types:

* [`actions`](https://www.osohq.com/docs/reference/api/check-api/post-actions)
* [`authorize`](https://www.osohq.com/docs/reference/api/check-api/post-authorize)
* [`evaluate_query`](https://www.osohq.com/docs/reference/api/check-api/post-evaluate_query)
* [`list`](https://www.osohq.com/docs/reference/api/check-api/post-list)

The query structure is similar to the [HTTP API](/app-integration/client-apis/api-explorer), with two additional required fields: `name` and `query_type`.

### Manifest Structure

```yaml  theme={null}
queries:
  - name: "Query name"
    query_type: <type>
    # Additional fields depend on query type
```

### Query Types

#### Authorize Query

```yaml  theme={null}
- name: <String> (must be unique)
  query_type: authorize
  actor_type: <String>
  actor_id: <String>
  action: <String>
  resource_type: <String>
  resource_id: <String>
```

#### List Query

```yaml  theme={null}
- name: <String> (unique)
  query_type: list
  actor_type: <String>
  actor_id: <String>
  action: <String>
  resource_type: <String>
```

#### Actions Query

```yaml  theme={null}
- name: <String> (unique)
  query_type: actions
  actor_type: <String>
  actor_id: <String>
  resource_type: <String>
  resource_id: <String>
```

#### Evaluate Query

```yaml  theme={null}
- name: <String>
  query_type: evaluate_query
  predicate: [<String>]
  calls:
    - [<String>]
  constraints:
    <String>:
      type: <String>
      ids: [<String>]
    <String>:
      type: <String>
```

**Required fields:**

* `predicate`: Array where first element is predicate name, the rest are variable names
* `constraints`: Map of variable names to constraint definitions
  * `type`: Entity type for the variable
  * `ids` (optional): List of IDs to test
* All variables in `predicate` or `calls` must be defined in `constraints`

**Optional fields:**

* `calls`: Array of predicate calls formatted like `predicate`

#### Context Facts

You can add context facts to any of the above queries. If you add a context fact to a query, each of these fields below are required:

```yaml  theme={null}
context_facts:
  - predicate: <String>
    args:
      - type: <String>
        id: <String>
      - type: <String>
        id: <String>
      - type: <String>
        id: <String>
```

## Best Practices

### Create Representative Query Sets

Include queries that represent:

* Common authorization paths
* Edge cases
* Performance-critical queries
* Recently changed policy rules

## Troubleshooting

### "Feature not enabled" error

Policy Preview must be enabled. Contact us on [Slack](https://oso-oss.slack.com/ssb/redirect) to gain access.

### YAML parsing errors

Check your manifest structure. Common issues include:

* Missing required fields
* Incorrect indentation
* Unknown fields
