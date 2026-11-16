import * as cdk from 'aws-cdk-lib';
import { NAME_PREFIX, ProbeStack } from '../lib/probe-stack.ts';

const PLACEHOLDER_TAG = 'unset';

const app = new cdk.App();
const imageTag = app.node.tryGetContext('imageTag') ?? PLACEHOLDER_TAG;
const vpcId = app.node.tryGetContext('vpcId');

new ProbeStack(app, 'FargateIoUringPoc', {
  stackName: NAME_PREFIX,
  imageTag,
  vpcId,
  // VPC lookup needs a concrete environment. Otherwise stay env-agnostic so
  // the new VPC uses Fn::GetAZs and synth needs no AWS calls.
  env: vpcId
    ? { account: process.env.CDK_DEFAULT_ACCOUNT, region: process.env.CDK_DEFAULT_REGION }
    : undefined,
  // Legacy synthesis deploys the small template inline with the caller's
  // credentials: no CDK bootstrap stack is needed (there are no assets).
  synthesizer: new cdk.LegacyStackSynthesizer(),
});

cdk.Tags.of(app).add('Project', NAME_PREFIX);
cdk.Tags.of(app).add('Purpose', 'io_uring-on-fargate-experiment');
