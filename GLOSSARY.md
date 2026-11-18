# Strata

Strata is a backend service that indexes and searches the documents of many organisations in one system.

## Language

**Tenant**:
An organisation whose documents Strata keeps. It is a group of several people, and it is the boundary that no data crosses.
_Avoid_: Customer, organisation, workspace, company

**Account**:
The identity of one person inside exactly one tenant. A person who works with two tenants has one account in each.
_Avoid_: User, member, login

**Principal**:
The actor of an action. It is an account, or a job that Strata runs for one tenant.
_Avoid_: Subject, caller, actor

**Action**:
One operation that a principal requests, for example a search or an upload.
_Avoid_: Permission, operation, verb

**Authorized scope**:
The set of documents and other data that a principal can reach with one action. A search reads only inside this set.
_Avoid_: Filter, visibility, access list

**Design ceiling**:
The peak load that one deployment of Strata must be able to serve. It is a test target, not a forecast.
_Avoid_: Capacity, limit, forecast

**Concurrent account**:
An account that sends requests in the peak period. It is not the same as an active account: "active" is the status of an account.
_Avoid_: Active user, online user, session

**Job**:
One unit of work that a worker does later for one tenant, for example the processing of an uploaded revision.
_Avoid_: Task, message, event

**Parked job**:
A job that stopped and waits for an operator. A job becomes parked after its last failed attempt, or when its tenant is not active.
_Avoid_: Dead letter, failed job
