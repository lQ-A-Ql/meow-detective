import { CaseWelcomeForms } from '../components/CaseWelcomeForms';
import type { CaseWelcomeFormsProps } from '../components/case-welcome-types';
import { useCaseWelcomeState } from '../use-case-welcome-state';

export function CaseWelcomeContainer(props: CaseWelcomeFormsProps) {
  const state = useCaseWelcomeState(props);
  return <CaseWelcomeForms {...props} state={state} />;
}
