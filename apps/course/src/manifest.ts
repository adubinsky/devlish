export interface Example {
  filename: string;
  source: string;
}

export interface Fixture {
  filename: string;
  content: string;
}

export interface Exercise {
  id: string;
  title: string;
  description: string;
  starterCode: string;
  expected: {
    outputs?: string[];
    variables?: Record<string, unknown>;
  };
  hints?: string[];
}

export interface DvtScenario {
  description: string;
  file: string;
  method?: string;
  args?: unknown[];
  expected: {
    outputs?: string[];
    variables?: Record<string, unknown>;
  };
}

export interface Lesson {
  id: string;
  title: string;
  markdown: string;
  examples: Example[];
  fixtures: Fixture[];
  exercises: Exercise[];
  dvtScenarios: DvtScenario[];
}

export interface Chapter {
  id: string;
  title: string;
  description: string;
  lessons: Lesson[];
}

export interface Course {
  title: string;
  chapters: Chapter[];
}

import courseData from "./course-data.json";

export function getCourse(): Course {
  return courseData as Course;
}
