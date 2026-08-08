---
name: aws-compliance
description: Use when reviewing ADRs or IaC files for AWS Well-Architected Framework compliance. Checks AWS resources against the five pillars and produces a structured report.
---

# AWS compliance

Review ADRs and IaC files for compliance with the AWS Well-Architected Framework. Report; do
not fix.

This skill owns the procedure for checking AWS resource compliance. It is invoked by agents
that need to review AWS infrastructure (e.g. `aws-architect`) but does not own the decision of
*when* to review — that belongs to the calling agent or the user.

**Authority:** AWS Well-Architected Framework (external standard, not project guidelines).

## Not this skill's job

| Question | Skill that owns it |
| --- | --- |
| Does the code follow project guidelines? | `review-standards` — this skill checks AWS compliance, not project rules |
| Is the architecture sound and consistent with ADRs? | `architecture-review` — this skill checks AWS pillars, not module structure |
| Does the IaC file have syntax errors? | The IaC tool itself (Terraform, CloudFormation) — this skill checks compliance, not syntax |

## 1. Identify the scope

Determine what is under review:

- An ADR in `docs/adrs/` that proposes AWS infrastructure changes.
- IaC files (CloudFormation, Terraform, CDK) that have been modified.
- Both, if the user requests a comprehensive review.

State the scope in the report header.

## 2. Check if AWS

For each file under review, verify it references AWS resources:

- **ADRs:** Look for mentions of AWS services (EC2, S3, RDS, Lambda, etc.), infrastructure, or
  cloud deployments.
- **IaC files:** Check file extensions (`.yaml`, `.yml`, `.tf`, `.tf.json`, `.ts`, `.py`, `.js`)
  and content for AWS resource definitions (`aws_*`, `AWS::*`, CDK constructs).

Skip non-AWS files. Report them as "Skipped — not AWS" with a one-line reason.

If no AWS resources are found, stop and report: "No AWS resources found in scope."

## 3. Read the files

Read the full content of every ADR and IaC file under review. Do not skim. Do not work from
memory or file names.

For IaC files, identify:

- Every AWS resource defined (e.g. `aws_instance`, `AWS::EC2::Instance`).
- Resource configurations (instance types, security groups, IAM policies, etc.).
- Dependencies between resources.

## 4. Check against the five pillars

For each AWS resource, evaluate against the five pillars of the AWS Well-Architected Framework:

### Security

- No hardcoded credentials in IaC files or ADRs.
- Uses IAM roles and policies with least-privilege permissions.
- Encryption at rest (S3, RDS, EBS) and in transit (TLS/SSL).
- Security groups restrict inbound traffic to necessary ports only.
- No public access to sensitive resources (databases, internal services).

### Reliability

- Multi-AZ or multi-region deployment where appropriate.
- Health checks configured for load balancers and auto-scaling groups.
- Auto-recovery mechanisms (auto-scaling, failover).
- Failure handling and retry logic defined.
- Dependency degradation strategies (circuit breakers, fallbacks).

### Performance Efficiency

- Right-sized resources (instance types match workload).
- Auto-scaling configured for variable workloads.
- Caching used where appropriate (ElastiCache, CloudFront).
- Serverless where appropriate (Lambda, Fargate).
- Monitoring and metrics configured (CloudWatch).

### Cost Optimization

- Resource tagging for cost allocation.
- Right-sizing (no over-provisioned instances).
- Reserved instances or Savings Plans for steady-state workloads.
- Spend monitoring and alerts configured.
- Waste elimination (unused resources, idle load balancers).

### Operational Excellence

- Infrastructure as code (all resources in IaC files, not manual).
- Deployment automation (CI/CD pipelines, automated rollbacks).
- Monitoring and alerting configured (CloudWatch alarms, SNS notifications).
- Runbooks or operational procedures documented.
- Incident response procedures defined.

## 5. Produce findings

For each pillar and each AWS resource, record:

- **Pass/Fail:** Whether the resource passes the pillar check.
- **Note:** A brief explanation of what was checked and what was found.
- **Recommendation:** If the check failed, a specific recommendation to fix it.

Use this format:

```markdown
### Security

**aws_instance.web_server**
- **Status:** Fail
- **Note:** Security group allows inbound SSH (port 22) from 0.0.0.0/0.
- **Recommendation:** Restrict SSH access to specific IP ranges or use AWS Systems Manager
  Session Manager instead.

**aws_s3_bucket.data**
- **Status:** Pass
- **Note:** Bucket encryption enabled (AES-256). Public access blocked.
- **Recommendation:** —
```

## 6. Produce the report

Emit exactly these sections, in this order:

```markdown
## AWS compliance review

**Scope:** <ADRs and IaC files reviewed>
**AWS resources found:** <count and list of resources>
**Non-AWS files skipped:** <list with reasons, or "none">

### Security
<findings per resource>

### Reliability
<findings per resource>

### Performance Efficiency
<findings per resource>

### Cost Optimization
<findings per resource>

### Operational Excellence
<findings per resource>

### Summary
<Pass/Fail count per pillar>
<one sentence: compliant, or what must change>
```

## 7. Stop

Report only. Do not edit ADRs, IaC files, or code. If the user then asks for fixes, apply them
one finding at a time.

## Red flags

| If you catch yourself... | Do this instead |
| --- | --- |
| Reviewing non-AWS files | Skip them and report "not AWS" |
| Claiming compliance without reading the file | Read the full content; do not work from file names |
| Confusing architectural preference with Well-Architected compliance | Stick to the five pillars; do not invent a sixth |
| Certifying compliance | This is a guideline review, not a guarantee — flag findings, do not certify |
| Reviewing from memory | Read the file; the AWS Well-Architected Framework changes over time |
| Editing IaC files while reviewing | Report first. Fixes are a separate, requested step |
