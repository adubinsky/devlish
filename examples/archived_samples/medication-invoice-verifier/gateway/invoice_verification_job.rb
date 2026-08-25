# frozen_string_literal: true

require 'sidekiq'
require 'json'

class InvoiceVerificationJob
  include Sidekiq::Job

  sidekiq_options queue: :medication_invoices, retry: 5

  def perform(payload)
    # Intended integration point:
    # invoke Devlish process execution with payload context and capture decision output.
    raise NotImplementedError, "Hook Devlish executor into InvoiceVerificationJob#perform"
  end
end
