export interface ResolvedEntity {
  id: string;
  entityType: string;
  canonicalValue: string;
  sourceEntities: string[];
  sourceCount: number;
  confidence: number;
  attributes: string[];
}

export interface EntityMergeResult {
  resolvedCount: number;
  mergedCount: number;
  resolved: ResolvedEntity[];
}

export type EntityRelationshipType =
  | 'communicatesWith'
  | 'owns'
  | 'loggedInto'
  | 'executed'
  | 'downloaded'
  | 'accessed';

export interface EntityRelationship {
  id: string;
  caseId: string;
  sourceEntityId: string;
  targetEntityId: string;
  relationshipType: EntityRelationshipType;
  confidence: number;
  evidenceEdgeIds: string[];
  createdAt: string;
}

export interface EntityRelationshipResult {
  relationshipCount: number;
  relationships: EntityRelationship[];
}
