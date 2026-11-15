# Strata

Strata is a backend service that indexes and searches the documents of many organisations in one system.

## Language

**Tenant**:
An organisation whose documents Strata keeps. It is a group of several people, and it is the boundary that no data crosses.
_Avoid_: Customer, organisation, workspace, company

**Account**:
The identity of one person inside exactly one tenant. A person who works with two tenants has one account in each.
_Avoid_: User, member, login
